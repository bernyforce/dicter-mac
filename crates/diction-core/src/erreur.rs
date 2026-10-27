//! Type d'erreur unique de l'app et journal défensif.

use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq)]
pub enum Erreur {
    Micro(String),
    PasserelleHorsLigne(String),
    Passerelle(String),
    Reponse(String),
    Collage(String),
    Config(String),
    Update(String),
    Llm(String),
    Son(String),
    Duck(String),
}

impl Erreur {
    /// Code stable à citer dans un rapport : il identifie l'étape qui a échoué.
    pub fn code(&self) -> &'static str {
        match self {
            Erreur::Micro(_) => "E-MIC",
            Erreur::PasserelleHorsLigne(_) => "E-NET-OFF",
            Erreur::Passerelle(_) => "E-NET-HTTP",
            Erreur::Reponse(_) => "E-NET-RESP",
            Erreur::Collage(_) => "E-PASTE",
            Erreur::Config(_) => "E-CFG",
            Erreur::Update(_) => "E-UPD",
            Erreur::Llm(_) => "E-LLM",
            Erreur::Son(_) => "E-SND",
            Erreur::Duck(_) => "E-DUCK",
        }
    }
}

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let code = self.code();
        match self {
            Erreur::Micro(m) => write!(f, "[{code}] Micro indisponible : {m}"),
            Erreur::PasserelleHorsLigne(m) => write!(f, "[{code}] Passerelle injoignable : {m}"),
            Erreur::Passerelle(m) => write!(f, "[{code}] La passerelle a refusé la requête : {m}"),
            Erreur::Reponse(m) => write!(f, "[{code}] Réponse illisible : {m}"),
            Erreur::Collage(m) => write!(
                f,
                "[{code}] Collage impossible (texte laissé dans le presse-papiers) : {m}"
            ),
            Erreur::Config(m) => write!(f, "[{code}] Réglage invalide : {m}"),
            Erreur::Update(m) => write!(f, "[{code}] Mise à jour : {m}"),
            Erreur::Llm(m) => write!(f, "[{code}] Mode Commande (texte inchangé) : {m}"),
            Erreur::Son(m) => write!(
                f,
                "[{code}] Effet sonore non joué (sans effet sur la dictée) : {m}"
            ),
            Erreur::Duck(m) => write!(
                f,
                "[{code}] Volume système non modifié (sans effet sur la dictée) : {m}"
            ),
        }
    }
}

impl From<ureq::Error> for Erreur {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(code, _) => Erreur::Passerelle(format!("HTTP {code}")),
            ureq::Error::Transport(t) => Erreur::PasserelleHorsLigne(t.to_string()),
        }
    }
}

/// Renomme les anciens fichiers `DictionMac*` de `dir` vers `DICTER*` s'ils existent et que le nouveau n'existe pas.
/// Rend les erreurs de renommage, sans jamais rien écraser.
fn migrate_old_files(dir: &std::path::Path) -> Vec<String> {
    let mut errs = Vec::new();
    for (old, new) in [
        ("DictionMac.log", "DICTER.log"),
        ("DictionMac-diagnostic.txt", "DICTER-diagnostic.txt"),
    ] {
        let (o, n) = (dir.join(old), dir.join(new));
        if o.exists()
            && !n.exists()
            && let Err(e) = std::fs::rename(&o, &n)
        {
            errs.push(format!("migration de {old} impossible : {e}"));
        }
    }
    errs
}

pub fn log_path() -> Option<std::path::PathBuf> {
    static MIGRATION: std::sync::Once = std::sync::Once::new();
    let dir = std::path::Path::new(&std::env::var_os("HOME")?).join("Library/Logs");
    let path = dir.join("DICTER.log");
    if !cfg!(test) {
        MIGRATION.call_once(|| {
            for e in migrate_old_files(&dir) {
                // Jamais bloquant : l'avertissement va dans le nouveau journal si possible.
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = writeln!(f, "[WARN] {e}");
                }
            }
        });
    }
    Some(path)
}

/// Plafond du journal : au-delà, l'ancien fichier passe en `.1` (une seule génération gardée).
const LOG_MAX_BYTES: u64 = 1_000_000;

/// 0 = error, 1 = info, 2 = debug.
static LOG_LEVEL: AtomicU8 = AtomicU8::new(1);

pub fn set_log_level(level: &str) {
    let n = match level {
        "error" => 0,
        "debug" => 2,
        _ => 1,
    };
    LOG_LEVEL.store(n, Ordering::Relaxed);
}

/// Remplace le journal par une copie `.1` s'il dépasse `max` octets.
fn rotate_if_big(path: &std::path::Path, max: u64) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > max) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
}

/// Horodatage Unix + cause. Jamais le texte dicté. Les messages « [E-… » sont toujours écrits,
/// les autres seulement si le niveau est « info » ou « debug ».
pub fn log(msg: &str) {
    let level = if msg.starts_with("[E-") { 0 } else { 1 };
    write_log(level, msg);
}

/// Détail de diagnostic, écrit seulement au niveau « debug ».
pub fn log_debug(msg: &str) {
    write_log(2, msg);
}

fn write_log(level: u8, msg: &str) {
    if cfg!(test) || level > LOG_LEVEL.load(Ordering::Relaxed) {
        return; // les tests ne doivent pas écrire dans le vrai journal
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Some(path) = log_path() else { return };
    rotate_if_big(&path, LOG_MAX_BYTES);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{ts} {msg}");
    }
}

/// Toute panique (thread principal ou autre) laisse sa cause dans le journal avant de s'arrêter.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let place = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "sans message".into());
        log(&format!("[E-PANIC] {msg} ({place})"));
    }));
}

/// Texte de diagnostic à joindre à un rapport. Contient versions, réglages et fin du journal, jamais le texte dicté.
pub fn diagnostic_text(
    version: &str,
    os: &str,
    accessibility: bool,
    config_json: &str,
    log_tail: &str,
) -> String {
    format!(
        "DICTER {version}\nmacOS {os}\nAccessibilité : {}\n\n== Réglages ==\n{config_json}\n\n== Journal (fin) ==\n{log_tail}\n",
        if accessibility {
            "accordée"
        } else {
            "refusée"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_erreur_a_un_code_dans_son_message() {
        let e = Erreur::Micro("x".into());
        assert_eq!(e.code(), "E-MIC");
        assert!(e.to_string().starts_with("[E-MIC]"));
        assert!(
            Erreur::PasserelleHorsLigne("x".into())
                .to_string()
                .contains("E-NET-OFF")
        );
    }

    #[test]
    fn le_code_e_snd_existe() {
        let e = Erreur::Son("afplay".into());
        assert_eq!(e.code(), "E-SND");
        assert!(e.to_string().starts_with("[E-SND]"));
    }

    #[test]
    fn le_code_e_duck_existe() {
        let e = Erreur::Duck("osascript".into());
        assert_eq!(e.code(), "E-DUCK");
        assert!(e.to_string().starts_with("[E-DUCK]"));
    }

    #[test]
    fn le_journal_tourne_au_dela_du_plafond() {
        let d = std::env::temp_dir().join(format!("diction-log-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("t.log");
        std::fs::write(&p, "x".repeat(50)).unwrap();
        rotate_if_big(&p, 100);
        assert!(p.exists());
        rotate_if_big(&p, 10);
        assert!(!p.exists() && d.join("t.log.1").exists());
        set_log_level("error");
        assert_eq!(LOG_LEVEL.load(Ordering::Relaxed), 0);
        set_log_level("debug");
        assert_eq!(LOG_LEVEL.load(Ordering::Relaxed), 2);
        set_log_level("info");
    }

    #[test]
    fn la_migration_renomme_sans_ecraser() {
        let d = std::env::temp_dir().join(format!("dicter-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("DictionMac.log"), "ancien").unwrap();
        std::fs::write(d.join("DictionMac-diagnostic.txt"), "ancien diag").unwrap();
        std::fs::write(d.join("DICTER-diagnostic.txt"), "nouveau diag").unwrap();
        assert!(migrate_old_files(&d).is_empty());
        assert_eq!(
            std::fs::read_to_string(d.join("DICTER.log")).unwrap(),
            "ancien"
        );
        assert!(!d.join("DictionMac.log").exists());
        // le nouveau existait déjà : rien n'est écrasé, l'ancien reste
        assert_eq!(
            std::fs::read_to_string(d.join("DICTER-diagnostic.txt")).unwrap(),
            "nouveau diag"
        );
        assert!(d.join("DictionMac-diagnostic.txt").exists());
        // relance sans ancien fichier : rien ne casse
        assert!(migrate_old_files(&d.join("absent")).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn le_diagnostic_contient_les_sections() {
        let d = diagnostic_text("0.2.1", "26.6", false, "{}", "ligne");
        assert!(d.contains("DICTER 0.2.1") && d.contains("refusée") && d.contains("ligne"));
    }
}
