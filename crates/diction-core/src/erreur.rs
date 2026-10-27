//! Type d'erreur unique de l'app et journal défensif.

use std::fmt;
use std::io::Write;
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

pub fn log_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join("Library/Logs/DictionMac.log"))
}

/// Horodatage Unix + cause. Jamais le texte dicté.
pub fn log(msg: &str) {
    if cfg!(test) {
        return; // les tests ne doivent pas écrire dans le vrai journal
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Some(path) = log_path() else { return };
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
        "Diction {version}\nmacOS {os}\nAccessibilité : {}\n\n== Réglages ==\n{config_json}\n\n== Journal (fin) ==\n{log_tail}\n",
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
    fn le_diagnostic_contient_les_sections() {
        let d = diagnostic_text("0.2.1", "26.6", false, "{}", "ligne");
        assert!(d.contains("Diction 0.2.1") && d.contains("refusée") && d.contains("ligne"));
    }
}
