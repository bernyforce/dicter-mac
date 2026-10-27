//! Livraison du texte : copie seule, ou collage ⌘V avec restauration du presse-papiers.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use diction_core::config::Config;
use diction_core::erreur::Erreur;

fn clipboard_set(text: &str) -> Result<(), Erreur> {
    let e = |e: std::io::Error| Erreur::Collage(e.to_string());
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(e)?;
    child
        .stdin
        .take()
        .ok_or_else(|| Erreur::Collage("pbcopy sans entrée".into()))?
        .write_all(text.as_bytes())
        .map_err(e)?;
    child.wait().map(|_| ()).map_err(e)
}

/// Frappe le texte caractère par caractère : lent, mais n'utilise pas le presse-papiers.
fn type_text(text: &str) -> Result<(), Erreur> {
    let ok = Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "tell application \"System Events\" to keystroke (item 1 of argv)",
            "-e",
            "end run",
        ])
        .arg(text)
        .status()
        .map_err(|e| Erreur::Collage(e.to_string()))?;
    if ok.success() {
        Ok(())
    } else {
        Err(Erreur::Collage(
            "autorisation Accessibilité manquante ?".into(),
        ))
    }
}

// ponytail: restaure seulement un presse-papiers texte; une image copiée avant est perdue.
pub fn deliver(text: &str, cfg: &Config) -> Result<(), Erreur> {
    if cfg.paste_method == "copy" {
        return clipboard_set(text);
    }
    if cfg.paste_method == "direct" {
        return type_text(text);
    }
    let old = Command::new("pbpaste")
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    clipboard_set(text)?;
    std::thread::sleep(Duration::from_millis(60));
    let ok = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to keystroke \"v\" using command down",
        ])
        .status()
        .map_err(|e| Erreur::Collage(e.to_string()))?;
    if !ok.success() {
        return Err(Erreur::Collage(
            "autorisation Accessibilité manquante ?".into(),
        ));
    }
    if cfg.restore_clipboard {
        std::thread::sleep(Duration::from_millis(cfg.paste_delay_ms));
        let _ = clipboard_set(&String::from_utf8_lossy(&old));
    }
    Ok(())
}
