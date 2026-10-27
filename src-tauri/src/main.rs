//! Diction pour Mac : dictée vocale via une passerelle Diction. Réglages dans la fenêtre « Diction »,
//! pastille d'enregistrement au-dessus des fenêtres. Journal : ~/Library/Logs/DictionMac.log

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use diction_audio::{self as audio, Cmd, Handlers};
use diction_core::config::{self, Config};
use diction_core::erreur::{self, Erreur, log};
use diction_core::history::History;
use diction_core::{format, net};
use diction_platform::{self as platform, paste};

use commands::SaveResult;

const OVERLAY_W: f64 = 240.0;
const OVERLAY_H: f64 = 56.0;
const MIN_SECONDS: f32 = 0.3;
const KEPT_AUDIO: usize = 5;

#[derive(Default)]
struct Shortcuts {
    main: Option<Shortcut>,
    paste_last: Option<Shortcut>,
    cancel: Option<Shortcut>,
}

pub(crate) struct Shared {
    cfg: Mutex<Config>,
    path: PathBuf,
    tx: mpsc::Sender<Cmd>,
    recording: AtomicBool,
    cancelled: AtomicBool,
    shortcut_error: Mutex<Option<String>>,
    shortcuts: Mutex<Shortcuts>,
    pub(crate) history: Mutex<History>,
    pub(crate) history_path: PathBuf,
    audio_dir: PathBuf,
}

pub(crate) fn current_cfg(app: &AppHandle) -> Config {
    app.state::<Shared>()
        .cfg
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default()
}

fn set_state(app: &AppHandle, s: &str) {
    let _ = app.emit("state", s);
}

fn show_overlay(app: &AppHandle, position: &str) {
    let Some(w) = app.get_webview_window("overlay") else {
        return;
    };
    if let Ok(Some(m)) = app.primary_monitor() {
        let s = m.scale_factor();
        let (mw, mh) = (m.size().width as f64 / s, m.size().height as f64 / s);
        let y = if position == "bottom" {
            mh - OVERLAY_H - 90.0
        } else {
            48.0
        };
        let _ = w.set_position(LogicalPosition::new((mw - OVERLAY_W) / 2.0, y));
    }
    let _ = w.set_ignore_cursor_events(true);
    let _ = w.show();
}

fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

fn show_main(app: &AppHandle, raison: &str) {
    log(&format!("fenêtre de réglages affichée ({raison})"));
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Dock affiché ou non. Sans Dock, la pastille peut passer au-dessus des apps en plein écran.
fn set_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}

fn start_recording(app: &AppHandle) {
    let sh = app.state::<Shared>();
    if sh.recording.swap(true, Ordering::SeqCst) {
        return;
    }
    let cfg = current_cfg(app);
    let _ = sh.tx.send(Cmd::Start(cfg.mic.clone()));
    // « Annuler » n'est actif que pendant l'enregistrement : il ne vole pas la touche le reste du temps.
    if let Some(c) = sh.shortcuts.lock().ok().and_then(|s| s.cancel) {
        let _ = app.global_shortcut().register(c);
    }
    if cfg.sounds {
        platform::play_sound("start", cfg.sound_volume);
    }
    if cfg.overlay {
        show_overlay(app, &cfg.overlay_position);
    }
    set_state(app, "recording");
}

fn release_cancel(app: &AppHandle) {
    if let Some(c) = app
        .state::<Shared>()
        .shortcuts
        .lock()
        .ok()
        .and_then(|s| s.cancel)
    {
        let _ = app.global_shortcut().unregister(c);
    }
}

fn stop_recording(app: &AppHandle) {
    let sh = app.state::<Shared>();
    if !sh.recording.swap(false, Ordering::SeqCst) {
        return;
    }
    release_cancel(app);
    let cfg = current_cfg(app);
    if cfg.sounds {
        platform::play_sound("stop", cfg.sound_volume);
    }
    let _ = sh.tx.send(Cmd::Stop);
    set_state(app, "processing");
}

/// Abandonne l'enregistrement en cours : l'audio est jeté, rien n'est envoyé.
fn cancel_recording(app: &AppHandle) {
    let sh = app.state::<Shared>();
    if !sh.recording.swap(false, Ordering::SeqCst) {
        return;
    }
    sh.cancelled.store(true, Ordering::SeqCst);
    release_cancel(app);
    let _ = sh.tx.send(Cmd::Stop);
    set_state(app, "idle");
    hide_overlay(app);
    log("enregistrement annulé");
}

/// Fin d'un cycle : retour au repos, ou erreur affichée 3 s puis retour au repos.
fn finish(app: &AppHandle, err: Option<Erreur>) {
    let Some(e) = err else {
        set_state(app, "idle");
        hide_overlay(app);
        return;
    };
    let msg = e.to_string();
    log(&msg);
    if current_cfg(app).overlay {
        let _ = app.emit("error", &msg);
        set_state(app, "error");
    } else {
        platform::notifier(&msg);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        if !app.state::<Shared>().recording.load(Ordering::SeqCst) {
            set_state(&app, "idle");
            hide_overlay(&app);
        }
    });
}

/// Garde les derniers enregistrements pour comprendre un échec de transcription.
fn save_audio(dir: &Path, wav: &[u8]) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if std::fs::create_dir_all(dir).is_err()
        || std::fs::write(dir.join(format!("dictee-{ts}.wav")), wav).is_err()
    {
        log("[E-CFG] enregistrement audio non conservé");
        return;
    }
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for old in files.iter().take(files.len().saturating_sub(KEPT_AUDIO)) {
        let _ = std::fs::remove_file(old);
    }
}

fn remember(app: &AppHandle, text: &str, cfg: &Config) {
    let sh = app.state::<Shared>();
    if let Ok(mut h) = sh.history.lock() {
        h.push(text);
        if cfg.history_enabled && h.save(&sh.history_path).is_err() {
            log("[E-CFG] historique non enregistré");
        }
    }
    let _ = app.emit("history", ());
}

fn handle_audio(app: &AppHandle, samples: Vec<i16>, rate: u32) {
    if app
        .state::<Shared>()
        .cancelled
        .swap(false, Ordering::SeqCst)
    {
        return;
    }
    if (samples.len() as f32) < rate as f32 * MIN_SECONDS {
        finish(app, None);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let cfg = current_cfg(&app);
        let wav = audio::wav(&samples, rate);
        if cfg.keep_audio {
            save_audio(&app.state::<Shared>().audio_dir, &wav);
        }
        let r = net::transcribe(&cfg, "dictee.wav", &wav).and_then(|t| {
            let text = format::apply(&t, &cfg.format);
            if text.is_empty() {
                return Ok(());
            }
            remember(&app, &text, &cfg);
            if cfg.auto_insert {
                paste::deliver(&text, &cfg)
            } else {
                Ok(())
            }
        });
        finish(&app, r.err());
    });
}

fn paste_last(app: &AppHandle) {
    let text = app
        .state::<Shared>()
        .history
        .lock()
        .ok()
        .and_then(|h| h.last().cloned());
    let Some(text) = text else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let cfg = current_cfg(&app);
        finish(&app, paste::deliver(&text, &cfg).err());
    });
}

fn on_shortcut(app: &AppHandle, sc: &Shortcut, state: ShortcutState) {
    let (main, last, cancel) = {
        let sh = app.state::<Shared>();
        let Ok(s) = sh.shortcuts.lock() else { return };
        (s.main, s.paste_last, s.cancel)
    };
    if Some(*sc) == main {
        let hold = current_cfg(app).mode == "hold";
        let recording = app.state::<Shared>().recording.load(Ordering::SeqCst);
        match (hold, state) {
            (true, ShortcutState::Pressed) => start_recording(app),
            (true, ShortcutState::Released) => stop_recording(app),
            (false, ShortcutState::Pressed) if recording => stop_recording(app),
            (false, ShortcutState::Pressed) => start_recording(app),
            (false, ShortcutState::Released) => {}
        }
    } else if state == ShortcutState::Pressed {
        if Some(*sc) == last {
            paste_last(app);
        } else if Some(*sc) == cancel {
            cancel_recording(app);
        }
    }
}

/// Un raccourci déjà pris par une autre app échoue sans bruit : on garde l'erreur pour l'afficher.
fn apply_shortcuts(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let parse = |s: &str| s.parse::<Shortcut>().map_err(|e| format!("« {s} » : {e}"));
    let result = (|| {
        let (main, last, cancel) = (
            parse(&cfg.shortcut)?,
            parse(&cfg.shortcut_paste_last)?,
            parse(&cfg.shortcut_cancel)?,
        );
        if let Ok(mut s) = app.state::<Shared>().shortcuts.lock() {
            *s = Shortcuts {
                main: Some(main),
                paste_last: Some(last),
                cancel: Some(cancel),
            };
        }
        let mut errs = Vec::new();
        for (name, sc) in [
            ("enregistrement", main),
            ("coller la dernière transcription", last),
        ] {
            if let Err(e) = gs.register(sc) {
                errs.push(format!("{name} : {e}"));
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs.join(" ; "))
        }
    })();
    if let Ok(mut s) = app.state::<Shared>().shortcut_error.lock() {
        *s = result.clone().err();
    }
    if let Err(e) = &result {
        log(&format!("raccourci refusé : {e}"));
    }
    result
}

/// Enregistre les réglages puis applique ce qui a changé (raccourcis, démarrage, Dock, historique).
pub(crate) fn apply_config(app: &AppHandle, cfg: Config) -> Result<SaveResult, String> {
    let sh = app.state::<Shared>();
    config::save(&sh.path, &cfg).map_err(|e| e.to_string())?;
    let old = match sh.cfg.lock() {
        Ok(mut c) => std::mem::replace(&mut *c, cfg.clone()),
        Err(_) => return Err("réglages verrouillés".into()),
    };
    let mut warning = None;
    if old.autostart != cfg.autostart
        && let Err(e) = platform::set_autostart(cfg.autostart)
    {
        log(&e.to_string());
        warning = Some(e.to_string());
    }
    if old.show_dock != cfg.show_dock {
        set_dock(app, cfg.show_dock);
    }
    if old.history_enabled && !cfg.history_enabled {
        let _ = std::fs::remove_file(&sh.history_path);
    }
    let shortcut_error = if (
        &old.shortcut,
        &old.shortcut_paste_last,
        &old.shortcut_cancel,
    ) != (
        &cfg.shortcut,
        &cfg.shortcut_paste_last,
        &cfg.shortcut_cancel,
    ) {
        apply_shortcuts(app, &cfg).err()
    } else {
        None
    };
    Ok(SaveResult {
        shortcut_error,
        warning,
    })
}

fn main() {
    // Vérification sans interface : diction-mac --file audio.wav
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, path] = args.as_slice()
        && flag == "--file"
    {
        let mut cfg = Config::default();
        if let Ok(u) = std::env::var("DICTION_URL") {
            cfg.url = u;
        }
        let r = std::fs::read(path)
            .map_err(|e| Erreur::Micro(e.to_string()))
            .and_then(|a| net::transcribe(&cfg, "fichier", &a));
        match r {
            Ok(t) => println!("{t}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    erreur::install_panic_hook();
    let built = tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, sc, ev| on_shortcut(app, sc, ev.state()))
                .build(),
        )
        .on_window_event(|w, e| {
            // Fermer la fenêtre de réglages la cache : l'app reste dans la barre de menus.
            if let WindowEvent::CloseRequested { api, .. } = e
                && w.label() == "main"
            {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::shortcut_status,
            commands::test_connection,
            commands::test_mic,
            commands::list_mics,
            commands::accessibility,
            commands::open_privacy,
            commands::read_log,
            commands::open_logs_folder,
            commands::export_diagnostic,
            commands::check_update,
            commands::open_url,
            commands::app_version,
            commands::get_history,
            commands::clear_history,
            commands::export_settings,
            commands::import_settings
        ])
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let path = dir.join("config.json");
            let history_path = dir.join("history.json");
            let cfg = config::load(&path);
            let history = if cfg.history_enabled {
                History::load(&history_path)
            } else {
                History::default()
            };
            let boot = cfg.clone();

            let (tx, rx) = mpsc::channel();
            app.manage(Shared {
                cfg: Mutex::new(cfg),
                path,
                tx,
                recording: AtomicBool::new(false),
                cancelled: AtomicBool::new(false),
                shortcut_error: Mutex::new(None),
                shortcuts: Mutex::new(Shortcuts::default()),
                history: Mutex::new(history),
                history_path,
                audio_dir: app.path().app_cache_dir()?.join("audio"),
            });
            set_dock(app.handle(), boot.show_dock);

            let (a_level, a_done, a_err) = (
                app.handle().clone(),
                app.handle().clone(),
                app.handle().clone(),
            );
            std::thread::spawn(move || {
                audio::run(
                    rx,
                    Handlers {
                        level: Box::new(move |l| {
                            let _ = a_level.emit("level", l);
                        }),
                        done: Box::new(move |s, r| handle_audio(&a_done, s, r)),
                        error: Box::new(move |e| {
                            a_err
                                .state::<Shared>()
                                .recording
                                .store(false, Ordering::SeqCst);
                            release_cancel(&a_err);
                            finish(&a_err, Some(e));
                        }),
                    },
                )
            });

            let _ = apply_shortcuts(app.handle(), &boot);

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "settings", "Réglages…", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?,
                ],
            )?;
            let mut tray =
                TrayIconBuilder::new()
                    .menu(&menu)
                    .on_menu_event(|app, ev| match ev.id.as_ref() {
                        "settings" => show_main(app, "menu"),
                        "quit" => app.exit(0),
                        _ => {}
                    });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if boot.auto_update_check && !boot.update_repo.is_empty() {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    match commands::check_update_blocking(&boot.update_repo) {
                        Ok(info) if info.newer => {
                            log(&format!("mise à jour disponible : {}", info.summary()));
                            let _ = h.emit("update", info);
                        }
                        Ok(_) => {}
                        Err(e) => log(&e.to_string()),
                    }
                });
            }
            show_main(app.handle(), "lancement");
            log(&format!("démarré, version {}", env!("CARGO_PKG_VERSION")));
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match built {
        Ok(app) => app,
        Err(e) => {
            log(&format!("[E-START] démarrage impossible : {e}"));
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        // Un clic sur l'icône de l'app alors qu'elle tourne déjà doit rouvrir la fenêtre.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main(app, "réouverture");
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}
