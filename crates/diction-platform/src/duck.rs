//! Baisse le volume système pendant la dictée, puis le remet. Jamais bloquant : une erreur s'écrit
//! dans le journal ([E-DUCK]) et la dictée continue.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use diction_core::erreur::{Erreur, log};

/// Volumes avant et après la baisse (0 à 100).
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Ducked {
    pub prev: u8,
    pub ducked: u8,
}

/// Fichier filet de sécurité : survit à un plantage et permet de restaurer au prochain lancement.
static PENDING: OnceLock<PathBuf> = OnceLock::new();

/// À appeler une fois au démarrage avec le dossier de configuration.
pub fn init(dir: &Path) {
    let _ = PENDING.set(dir.join("duck-pending"));
}

/// Nouveau volume : `percent` % du volume actuel.
pub fn target_volume(prev: u8, percent: u8) -> u8 {
    (u16::from(prev.min(100)) * u16::from(percent.min(100)) / 100) as u8
}

/// On ne restaure que si le volume est encore celui que nous avons posé ; sinon l'utilisateur l'a changé.
/// Tolérance de 1 point : macOS arrondit parfois le volume relu.
pub fn should_restore(current: u8, ducked: u8) -> bool {
    current.abs_diff(ducked) <= 1
}

fn err(m: impl ToString) {
    log(&Erreur::Duck(m.to_string()).to_string());
}

fn osa(script: &str) -> Option<String> {
    match Command::new("osascript").args(["-e", script]).output() {
        Ok(o) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        Ok(o) => {
            err(format!("osascript a rendu {}", o.status));
            None
        }
        Err(e) => {
            err(e);
            None
        }
    }
}

/// Volume de sortie actuel. `None` si la sortie est muette ou sans volume réglable (« missing value »).
fn current_volume() -> Option<u8> {
    let s = osa(
        "set s to get volume settings\n(output volume of s as text) & \",\" & (output muted of s as text)",
    )?;
    let (v, muted) = s.split_once(',')?;
    if muted.trim() == "true" {
        return None;
    }
    v.trim().parse::<u8>().ok()
}

fn set_volume(n: u8) {
    osa(&format!("set volume output volume {}", n.min(100)));
}

fn pending_text(d: &Ducked) -> String {
    format!("{} {}", d.prev, d.ducked)
}

fn parse_pending(s: &str) -> Option<Ducked> {
    let (a, b) = s.trim().split_once(' ')?;
    Some(Ducked {
        prev: a.parse().ok()?,
        ducked: b.parse().ok()?,
    })
}

fn write_pending(d: &Ducked) {
    let Some(p) = PENDING.get() else { return };
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let r = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(p)
        .and_then(|mut f| f.write_all(pending_text(d).as_bytes()));
    if let Err(e) = r {
        err(e);
    }
}

/// Baisse le volume à `percent` % de l'actuel. `None` si rien n'a été changé (muet, déjà plus bas, erreur).
pub fn begin(percent: u8) -> Option<Ducked> {
    let prev = current_volume()?;
    let ducked = target_volume(prev, percent);
    if ducked >= prev {
        return None;
    }
    let d = Ducked { prev, ducked };
    write_pending(&d); // avant la baisse : si l'app plante, le prochain lancement restaure
    set_volume(ducked);
    Some(d)
}

/// Remet le volume d'origine si l'utilisateur n'y a pas touché, puis supprime le filet de sécurité.
pub fn restore(d: &Ducked) {
    if current_volume().is_none_or(|c| should_restore(c, d.ducked)) {
        set_volume(d.prev);
    }
    if let Some(p) = PENDING.get() {
        let _ = std::fs::remove_file(p);
    }
}

/// Restaure depuis le fichier filet de sécurité (lancement, panique, arrêt). Sans fichier : rien.
pub fn restore_pending() {
    let Some(p) = PENDING.get() else { return };
    if let Some(d) = std::fs::read_to_string(p)
        .ok()
        .and_then(|s| parse_pending(&s))
    {
        restore(&d);
    }
    let _ = std::fs::remove_file(p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_cible_aux_bornes() {
        assert_eq!(target_volume(0, 20), 0);
        assert_eq!(target_volume(100, 20), 20);
        assert_eq!(target_volume(100, 0), 0);
        assert_eq!(target_volume(100, 80), 80);
        assert_eq!(target_volume(50, 20), 10);
    }

    #[test]
    fn on_ne_restaure_pas_un_volume_change_par_l_utilisateur() {
        assert!(should_restore(20, 20));
        assert!(should_restore(21, 20));
        assert!(!should_restore(60, 20));
        assert!(!should_restore(0, 20));
    }

    #[test]
    fn le_fichier_de_securite_fait_l_aller_retour() {
        let d = Ducked {
            prev: 70,
            ducked: 14,
        };
        assert_eq!(parse_pending(&pending_text(&d)), Some(d));
        assert_eq!(parse_pending("n'importe quoi"), None);
    }
}
