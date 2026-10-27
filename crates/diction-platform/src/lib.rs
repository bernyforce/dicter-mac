//! Tout ce qui parle au système d'exploitation. Seul module à adapter pour Windows et Linux.

pub mod duck;
pub mod paste;

use std::process::Command;

use diction_core::erreur::{Erreur, log};

/// Notification système. Le message passe en argument d'osascript, sans interpolation.
pub fn notifier(msg: &str) {
    let _ = Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 1 of argv) with title \"Diction\"",
            "-e",
            "end run",
        ])
        .arg(msg)
        .status();
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// L'autorisation Accessibilité est-elle accordée ? (nécessaire pour coller)
pub fn accessibility_granted() -> bool {
    // SAFETY: appel sans argument ni effet de bord, fourni par macOS.
    #[cfg(target_os = "macos")]
    unsafe {
        AXIsProcessTrusted()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Ouvre le volet de confidentialité demandé ("accessibility" ou "microphone").
pub fn open_privacy(pane: &str) {
    let anchor = match pane {
        "accessibility" => "Privacy_Accessibility",
        "microphone" => "Privacy_Microphone",
        _ => return,
    };
    let _ = Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{anchor}"
        ))
        .status();
}

pub fn os_version() -> String {
    Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Révèle un fichier dans le Finder.
pub fn reveal(path: &std::path::Path) {
    let _ = Command::new("open").arg("-R").arg(path).status();
}

/// Ouvre une page web, mais seulement sur github.com (les adresses viennent d'une réponse réseau).
pub fn open_url(url: &str) {
    if url.starts_with("https://github.com/") {
        let _ = Command::new("open").arg(url).status();
    }
}

/// Fichier système de l'effet. `kind` : "start" ou "stop".
fn sound_file(kind: &str) -> &'static str {
    if kind == "start" {
        "/System/Library/Sounds/Tink.aiff"
    } else {
        "/System/Library/Sounds/Pop.aiff"
    }
}

/// Joue un effet sonore sans attendre sa fin et sans jamais bloquer la dictée.
/// Un échec s'écrit dans le journal ([E-SND]). Le thread attend le processus pour ne pas laisser de zombie.
pub fn play_sound(kind: &str, volume: u8) {
    let spawned = Command::new("afplay")
        .arg("-v")
        .arg(format!("{:.2}", f32::from(volume.min(100)) / 100.0))
        .arg(sound_file(kind))
        .spawn();
    match spawned {
        Ok(mut child) => {
            std::thread::spawn(move || match child.wait() {
                Ok(s) if !s.success() => {
                    log(&Erreur::Son(format!("afplay a rendu {s}")).to_string())
                }
                Err(e) => log(&Erreur::Son(e.to_string()).to_string()),
                _ => {}
            });
        }
        Err(e) => log(&Erreur::Son(e.to_string()).to_string()),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn autostart_plist(exe: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>com.bf.diction-mac</string>\n<key>ProgramArguments</key><array><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
        xml_escape(exe)
    )
}

/// Lancement à l'ouverture de session : un LaunchAgent écrit ou supprimé.
pub fn set_autostart(enabled: bool) -> Result<(), diction_core::erreur::Erreur> {
    use diction_core::erreur::Erreur;
    let home = std::env::var_os("HOME").ok_or_else(|| Erreur::Config("HOME introuvable".into()))?;
    let path = std::path::Path::new(&home).join("Library/LaunchAgents/com.bf.diction-mac.plist");
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(Erreur::Config(e.to_string()))
            }
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| Erreur::Config(e.to_string()))?;
    std::fs::write(&path, autostart_plist(&exe.to_string_lossy()))
        .map_err(|e| Erreur::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_plist_de_demarrage_echappe_le_chemin() {
        let p = autostart_plist("/Apps/A&B/diction");
        assert!(p.contains("A&amp;B") && p.contains("RunAtLoad"));
    }

    #[test]
    fn chaque_effet_a_son_fichier() {
        assert!(sound_file("start").ends_with("Tink.aiff"));
        assert!(sound_file("stop").ends_with("Pop.aiff"));
    }

    #[test]
    fn la_version_du_systeme_est_lisible() {
        assert!(os_version().contains('.'));
    }
}
