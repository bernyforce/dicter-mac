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

fn one_line(text: &str, on: bool) -> String {
    if on {
        text.replace(['\r', '\n'], " ")
    } else {
        text.to_string()
    }
}

/// Touches du collage selon le mode : « ⌘V » (standard) ou « ⌃⇧V » (terminal). None = pas de touches.
fn paste_keys(method: &str) -> Option<&'static str> {
    match method {
        "paste" => Some("keystroke \"v\" using command down"),
        "terminal" => Some("keystroke \"v\" using {control down, shift down}"),
        _ => None,
    }
}

fn press(script: &str) -> Result<(), Erreur> {
    let ok = Command::new("osascript")
        .args([
            "-e",
            &format!("tell application \"System Events\" to {script}"),
        ])
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
    let text = &one_line(text, cfg.single_line);
    if cfg.paste_method == "copy" {
        return clipboard_set(text);
    }
    if cfg.paste_method == "direct" {
        // Échec de la frappe : le texte reste dans le presse-papiers (repli), l'erreur est journalisée par l'appelant.
        if let Err(e) = type_text(text) {
            let _ = clipboard_set(text);
            return Err(e);
        }
        return if cfg.copy_to_clipboard {
            clipboard_set(text)
        } else {
            Ok(())
        };
    }
    let Some(keys) = paste_keys(&cfg.paste_method) else {
        return clipboard_set(text);
    };
    let old = Command::new("pbpaste")
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    clipboard_set(text)?;
    std::thread::sleep(Duration::from_millis(60));
    // Échec : le texte est déjà dans le presse-papiers, on ne restaure pas l'ancien contenu.
    press(keys)?;
    if cfg.restore_clipboard && !cfg.copy_to_clipboard {
        std::thread::sleep(Duration::from_millis(cfg.paste_delay_ms));
        let _ = clipboard_set(&String::from_utf8_lossy(&old));
    }
    Ok(())
}

/// Copie la sélection courante (⌘C) et remet le presse-papiers d'avant. Vide = rien de sélectionné.
// ponytail: si Ctrl/Maj sont encore tenus, ⌘C peut être détourné; l'appelant attend avant d'appeler.
pub fn copy_selection() -> Result<String, Erreur> {
    let old = Command::new("pbpaste")
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    clipboard_set("")?;
    let ok = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to keystroke \"c\" using command down",
        ])
        .status()
        .map_err(|e| Erreur::Collage(e.to_string()))?;
    std::thread::sleep(Duration::from_millis(150));
    let sel = Command::new("pbpaste")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let _ = clipboard_set(&String::from_utf8_lossy(&old));
    if !ok.success() {
        return Err(Erreur::Collage(
            "autorisation Accessibilité manquante ?".into(),
        ));
    }
    Ok(sel)
}

#[cfg(test)]
mod tests {
    use super::{one_line, paste_keys};

    #[test]
    fn touches_selon_le_mode() {
        assert!(paste_keys("paste").unwrap().contains("command down"));
        let t = paste_keys("terminal").unwrap();
        assert!(t.contains("control down") && t.contains("shift down"));
        assert_eq!(paste_keys("direct"), None);
        assert_eq!(paste_keys("copy"), None);
    }

    #[test]
    fn retours_a_la_ligne_remplaces() {
        assert_eq!(one_line("a\r\nb\nc", true), "a  b c");
        assert_eq!(one_line("a\nb", false), "a\nb");
    }
}
