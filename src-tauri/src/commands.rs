//! Commandes appelées par l'interface (fenêtre de réglages).

use serde::Serialize;
use tauri::{AppHandle, Manager};

use diction_audio as audio;
use diction_core::config::{Config, host_is_local, host_of};
use diction_core::erreur::{self, Erreur, log};
use diction_core::{net, update};
use diction_platform as platform;

use crate::{Shared, apply_config, current_cfg};

#[derive(Serialize)]
pub struct SaveResult {
    pub shortcut_error: Option<String>,
    pub warning: Option<String>,
}

#[tauri::command]
pub fn get_config(app: AppHandle) -> Config {
    current_cfg(&app)
}

/// Réglages d'usine : source unique des valeurs de « Rétablir le défaut ».
#[tauri::command]
pub fn default_config() -> Config {
    Config::default()
}

#[tauri::command]
pub fn save_config(app: AppHandle, cfg: Config) -> Result<SaveResult, String> {
    apply_config(&app, cfg)
}

#[tauri::command]
pub fn shortcut_status(app: AppHandle) -> Option<String> {
    app.state::<Shared>()
        .shortcut_error
        .lock()
        .ok()
        .and_then(|s| s.clone())
}

#[derive(Serialize)]
pub struct Conn {
    models: Vec<String>,
    ms: u128,
}

#[tauri::command]
pub async fn test_connection(url: String) -> Result<Conn, String> {
    let r = tauri::async_runtime::spawn_blocking(move || net::models(&url))
        .await
        .map_err(|e| e.to_string())?;
    r.map(|(models, ms)| Conn { models, ms })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_mic(mic: String) -> Result<(), String> {
    let r = tauri::async_runtime::spawn_blocking(move || audio::probe(&mic))
        .await
        .map_err(|e| e.to_string())?;
    r.map_err(|e| e.to_string())
}

/// Joue l'effet « start » ou « stop » au volume demandé, pour le bouton « Tester ».
#[tauri::command]
pub fn test_sound(kind: String, volume: u8) {
    platform::play_sound(&kind, volume);
}

#[tauri::command]
pub fn list_mics() -> Vec<String> {
    audio::input_names()
}

#[tauri::command]
pub fn accessibility() -> bool {
    platform::accessibility_granted()
}

#[tauri::command]
pub fn open_privacy(pane: String) {
    platform::open_privacy(&pane);
}

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    current: String,
    latest: String,
    url: String,
    pub newer: bool,
}

pub fn check_update_blocking(repo: &str) -> Result<UpdateInfo, Erreur> {
    let rel = update::latest(repo)?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    Ok(UpdateInfo {
        newer: update::is_newer(&current, &rel.tag),
        current,
        latest: rel.tag,
        url: rel.url,
    })
}

impl UpdateInfo {
    pub fn summary(&self) -> String {
        format!("{} -> {}", self.current, self.latest)
    }
}

#[tauri::command]
pub async fn check_update(repo: String) -> Result<UpdateInfo, String> {
    let r = tauri::async_runtime::spawn_blocking(move || check_update_blocking(&repo))
        .await
        .map_err(|e| e.to_string())?;
    r.map_err(|e| {
        log(&e.to_string());
        e.to_string()
    })
}

#[tauri::command]
pub fn open_url(url: String) {
    platform::open_url(&url);
}

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn read_log() -> String {
    let Some(text) = erreur::log_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(60)..].join("\n")
}

#[tauri::command]
pub fn open_logs_folder() {
    if let Some(p) = erreur::log_path() {
        platform::reveal(&p);
    }
}

#[tauri::command]
pub fn export_diagnostic(app: AppHandle) -> Result<String, String> {
    let cfg = serde_json::to_string_pretty(&current_cfg(&app)).unwrap_or_default();
    let text = erreur::diagnostic_text(
        env!("CARGO_PKG_VERSION"),
        &platform::os_version(),
        platform::accessibility_granted(),
        &cfg,
        &read_log(),
    );
    let path = erreur::log_path()
        .and_then(|p| p.parent().map(|d| d.join("DICTER-diagnostic.txt")))
        .ok_or("dossier des journaux introuvable")?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    platform::reveal(&path);
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn get_history(app: AppHandle) -> Vec<String> {
    app.state::<Shared>()
        .history
        .lock()
        .map(|h| h.0.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_history(app: AppHandle) {
    let sh = app.state::<Shared>();
    if let Ok(mut h) = sh.history.lock() {
        h.clear();
        let _ = std::fs::remove_file(&sh.history_path);
    }
}

/// Exporte les réglages vers ~/Downloads. Le nom du micro n'est jamais exporté.
#[tauri::command]
pub fn export_settings(app: AppHandle) -> Result<String, String> {
    let cfg = Config {
        mic: String::new(),
        ..current_cfg(&app)
    };
    let home = std::env::var_os("HOME").ok_or("HOME introuvable")?;
    let path = std::path::Path::new(&home).join("Downloads/diction-settings.json");
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    platform::reveal(&path);
    Ok(path.display().to_string())
}

/// Importe un fichier de réglages. Le micro actuel est conservé.
#[tauri::command]
pub fn import_settings(app: AppHandle, path: String) -> Result<Config, String> {
    let text = std::fs::read_to_string(path.trim()).map_err(|e| e.to_string())?;
    let imported =
        Config::from_json(&text).map_err(|_| "fichier de réglages illisible".to_string())?;
    let current = current_cfg(&app);
    for (new, old) in [
        (&imported.url, &current.url),
        (&imported.command_url, &current.command_url),
    ] {
        if !host_is_local(new) && host_of(new) != host_of(old) {
            return Err(
                "hôte distant refusé à l'import : règle-le à la main dans Général".to_string(),
            );
        }
    }
    let cfg = Config {
        mic: current.mic,
        ..imported
    };
    cfg.validate().map_err(|e| e.to_string())?;
    apply_config(&app, cfg.clone())?;
    Ok(cfg)
}
