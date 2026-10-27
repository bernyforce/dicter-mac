//! Diction pour Mac : dictée vocale via une passerelle Diction. Réglages dans la fenêtre « Diction »,
//! pastille d'enregistrement au-dessus des fenêtres. Journal : ~/Library/Logs/DictionMac.log

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use diction_audio::{self as audio, Cmd, Handlers};
use diction_core::config::{self, Config};
use diction_core::erreur::{self, Erreur, log};
use diction_core::history::History;
use diction_core::{commande, format, net};
use diction_platform::{self as platform, duck, paste};

use commands::SaveResult;

const OVERLAY_W: f64 = 240.0;
const OVERLAY_H: f64 = 56.0;
const MIN_SECONDS: f32 = 0.3;
const KEPT_AUDIO: usize = 5;
/// Durée maximale d'un enregistrement, en secondes.
const MAX_SECONDS: u64 = 300;

#[derive(Default)]
struct Shortcuts {
    main: Option<Shortcut>,
    paste_last: Option<Shortcut>,
    cancel: Option<Shortcut>,
    command: Option<Shortcut>,
}

pub(crate) struct Shared {
    cfg: Mutex<Config>,
    path: PathBuf,
    tx: mpsc::Sender<Cmd>,
    recording: AtomicBool,
    /// Numéro de l'enregistrement en cours : un ancien minuteur ne touche pas un nouvel enregistrement.
    session: AtomicU64,
    cancelled: AtomicBool,
    /// Vrai si l'enregistrement en cours est une instruction du Mode Commande.
    command: AtomicBool,
    shortcut_error: Mutex<Option<String>>,
    shortcuts: Mutex<Shortcuts>,
    pub(crate) history: Mutex<History>,
    pub(crate) history_path: PathBuf,
    audio_dir: PathBuf,
    /// Volume d'origine si on l'a baissé pour cet enregistrement ; `restore_volume` le remet.
    ducked: Mutex<Option<duck::Ducked>>,
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

fn show_overlay(app: &AppHandle, cfg: &Config) {
    let (position, k) = (cfg.overlay_position.as_str(), cfg.overlay_scale());
    let (ow, oh) = (OVERLAY_W * k, OVERLAY_H * k);
    let Some(w) = app.get_webview_window("overlay") else {
        return;
    };
    if let Ok(Some(m)) = app.primary_monitor() {
        let s = m.scale_factor();
        let (mw, mh) = (m.size().width as f64 / s, m.size().height as f64 / s);
        let y = if position == "bottom" {
            mh - oh - 90.0
        } else {
            48.0
        };
        let _ = w.set_position(LogicalPosition::new((mw - ow) / 2.0, y));
    }
    let _ = w.set_size(LogicalSize::new(ow, oh));
    let _ = w.eval(format!("document.documentElement.style.zoom={k}"));
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

/// Remet le volume système s'il a été baissé. Idempotent : peut être appelé de partout.
fn restore_volume(app: &AppHandle) {
    if let Ok(mut g) = app.state::<Shared>().ducked.lock()
        && let Some(d) = g.take()
    {
        duck::restore(&d);
    }
}

fn start_recording(app: &AppHandle) {
    let sh = app.state::<Shared>();
    if sh.recording.swap(true, Ordering::SeqCst) {
        return;
    }
    let cfg = current_cfg(app);
    erreur::log_debug("enregistrement démarré");
    let _ = sh.tx.send(Cmd::Start(cfg.mic.clone()));
    // « Annuler » n'est actif que pendant l'enregistrement : il ne vole pas la touche le reste du temps.
    if let Some(c) = sh.shortcuts.lock().ok().and_then(|s| s.cancel) {
        let _ = app.global_shortcut().register(c);
    }
    if cfg.sound_start {
        platform::play_sound("start", cfg.sound_volume);
    }
    if cfg.overlay {
        show_overlay(app, &cfg);
    }
    set_state(app, "recording");
    let id = sh.session.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    if cfg.duck_others {
        let app = app.clone();
        std::thread::spawn(move || {
            // Le son de début doit finir avant la baisse, sinon il serait joué trop bas.
            if cfg.sound_start {
                std::thread::sleep(Duration::from_millis(300));
            }
            let sh = app.state::<Shared>();
            // Le verrou couvre la baisse : un arrêt concurrent attend, puis restaure.
            if let Ok(mut g) = sh.ducked.lock()
                && sh.recording.load(Ordering::SeqCst)
                && sh.session.load(Ordering::SeqCst) == id
            {
                *g = duck::begin(cfg.duck_percent);
            }
        });
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(MAX_SECONDS));
        let sh = app.state::<Shared>();
        if autostop_due(
            sh.session.load(Ordering::SeqCst),
            id,
            sh.recording.load(Ordering::SeqCst),
        ) {
            log("enregistrement arrêté : durée maximale atteinte");
            platform::notifier("Enregistrement arrêté après 5 minutes (durée maximale)");
            stop_recording(&app);
        }
    });
}

/// Le minuteur arrête l'enregistrement seulement si c'est encore le sien.
fn autostop_due(current: u64, started: u64, recording: bool) -> bool {
    recording && current == started
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
    restore_volume(app);
    let cfg = current_cfg(app);
    if cfg.sound_stop {
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
    restore_volume(app);
    let _ = sh.tx.send(Cmd::Stop);
    set_state(app, "idle");
    hide_overlay(app);
    log("enregistrement annulé");
}

/// Fin d'un cycle : retour au repos, ou erreur affichée 3 s puis retour au repos.
fn finish(app: &AppHandle, err: Option<Erreur>) {
    restore_volume(app);
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
        if app.state::<Shared>().command.swap(false, Ordering::SeqCst) {
            finish(&app, run_command(&app, &cfg, &wav).err());
            return;
        }
        let mut shown = false;
        let r = net::transcribe(&cfg, "dictee.wav", &wav).and_then(|t| {
            let text = format::apply(&t, &cfg.format);
            if text.is_empty() {
                return Ok(());
            }
            remember(&app, &text, &cfg);
            // Le texte part vers la pastille seulement, jamais vers le journal.
            if cfg.show_result && cfg.overlay {
                let _ = app.emit("result", &text);
                shown = true;
            }
            if cfg.auto_insert {
                paste::deliver(&text, &cfg)
            } else {
                Ok(())
            }
        });
        if shown {
            std::thread::sleep(Duration::from_secs(u64::from(cfg.result_seconds)));
            // Un nouvel enregistrement a repris la pastille : il gère lui-même son état.
            if r.is_ok() && app.state::<Shared>().recording.load(Ordering::SeqCst) {
                return;
            }
        }
        finish(&app, r.err());
    });
}

/// Mode Commande : instruction dictée + sélection → modèle local → remplacement. Échec = texte intact.
fn run_command(app: &AppHandle, cfg: &Config, wav: &[u8]) -> Result<(), Erreur> {
    let instruction = net::transcribe(
        &Config {
            enhance: false,
            ..cfg.clone()
        },
        "commande.wav",
        wav,
    )?;
    if instruction.is_empty() {
        return Ok(());
    }
    // Laisse le temps de relâcher les touches du raccourci avant d'envoyer ⌘C.
    std::thread::sleep(Duration::from_millis(300));
    let selection = paste::copy_selection()?;
    let reply = commande::run(cfg, &instruction, &selection)?;
    remember(app, &reply, cfg);
    if selection.is_empty() {
        // Sans sélection : une réponse à lire, pas un texte à coller.
        paste::deliver(
            &reply,
            &Config {
                paste_method: "copy".into(),
                ..cfg.clone()
            },
        )?;
        platform::notifier("Réponse copiée dans le presse-papiers");
        Ok(())
    } else {
        paste::deliver(&reply, cfg)
    }
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
    let (main, last, cancel, command) = {
        let sh = app.state::<Shared>();
        let Ok(s) = sh.shortcuts.lock() else { return };
        (s.main, s.paste_last, s.cancel, s.command)
    };
    if Some(*sc) == main || Some(*sc) == command {
        let is_command = Some(*sc) == command;
        let sh = app.state::<Shared>();
        if state == ShortcutState::Pressed && !sh.recording.load(Ordering::SeqCst) {
            sh.command.store(is_command, Ordering::SeqCst);
        }
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
        // Un raccourci vide (« Effacer ») n'est pas enregistré.
        let parse_opt = |s: &str| {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                parse(s).map(Some)
            }
        };
        let main = parse(&cfg.shortcut)?;
        let (last, cancel) = (
            parse_opt(&cfg.shortcut_paste_last)?,
            parse_opt(&cfg.shortcut_cancel)?,
        );
        let command = if cfg.command_enabled {
            parse_opt(&cfg.shortcut_command)?
        } else {
            None
        };
        if let Ok(mut s) = app.state::<Shared>().shortcuts.lock() {
            *s = Shortcuts {
                main: Some(main),
                paste_last: last,
                cancel,
                command,
            };
        }
        let mut errs = Vec::new();
        let mut all = vec![("enregistrement", main)];
        if let Some(l) = last {
            all.push(("coller la dernière transcription", l));
        }
        if let Some(c) = command {
            all.push(("mode Commande", c));
        }
        for (name, sc) in all {
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
    erreur::set_log_level(&cfg.log_level);
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
        &old.shortcut_command,
        old.command_enabled,
    ) != (
        &cfg.shortcut,
        &cfg.shortcut_paste_last,
        &cfg.shortcut_cancel,
        &cfg.shortcut_command,
        cfg.command_enabled,
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
    // Une panique ne doit pas laisser le volume baissé : on restaure avant d'écrire le journal.
    let log_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        duck::restore_pending();
        log_hook(info);
    }));
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
            commands::default_config,
            commands::save_config,
            commands::shortcut_status,
            commands::test_connection,
            commands::test_mic,
            commands::test_sound,
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
            // Volume resté bas après un plantage ou un arrêt brutal : on le remet avant tout le reste.
            duck::init(&dir);
            duck::restore_pending();
            erreur::set_log_level(&cfg.log_level);
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
                session: AtomicU64::new(0),
                cancelled: AtomicBool::new(false),
                command: AtomicBool::new(false),
                shortcut_error: Mutex::new(None),
                shortcuts: Mutex::new(Shortcuts::default()),
                history: Mutex::new(history),
                history_path,
                audio_dir: app.path().app_cache_dir()?.join("audio"),
                ducked: Mutex::new(None),
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
        if let tauri::RunEvent::Exit = event {
            restore_volume(app);
            duck::restore_pending();
        }
        // Un clic sur l'icône de l'app alors qu'elle tourne déjà doit rouvrir la fenêtre.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main(app, "réouverture");
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}

#[cfg(test)]
mod tests {
    use super::autostop_due;

    #[test]
    fn minuteur_ne_touche_pas_un_autre_enregistrement() {
        assert!(autostop_due(3, 3, true));
        assert!(!autostop_due(4, 3, true));
        assert!(!autostop_due(3, 3, false));
    }
}
