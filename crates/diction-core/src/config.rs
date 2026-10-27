//! Réglages persistés en JSON. Un fichier illisible est mis de côté (.bad), jamais perdu en silence.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::erreur::{Erreur, log};

/// Règles de formatage appliquées au texte reçu, avant l'insertion.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FormatRules {
    /// Texte de N mots ou moins : sans majuscule ni ponctuation. 0 = désactivé.
    pub short_text_words: u8,
    pub space_before_qe: bool,
    pub trailing_space: bool,
    pub remove_hesitations: bool,
    /// Une règle par ligne : « chercher => remplacer ».
    pub custom_rules: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    pub url: String,
    pub model: String,
    pub language: String,
    pub enhance: bool,
    pub shortcut: String,
    pub mode: String,
    pub paste_method: String,
    pub restore_clipboard: bool,
    pub paste_delay_ms: u64,
    pub overlay: bool,
    pub overlay_position: String,
    pub update_repo: String,
    pub auto_update_check: bool,
    pub format: FormatRules,
    /// Mots à reconnaître correctement (noms, jargon). Envoyés comme indice au serveur.
    pub dictionary: Vec<String>,
    pub shortcut_paste_last: String,
    pub shortcut_cancel: String,
    /// Si faux, le texte n'est pas inséré : utiliser « Coller la dernière transcription ».
    pub auto_insert: bool,
    /// Nom du micro, vide = micro par défaut du système.
    pub mic: String,
    pub autostart: bool,
    pub show_dock: bool,
    pub sounds: bool,
    pub sound_volume: u8,
    pub history_enabled: bool,
    pub keep_audio: bool,
    /// Mode Commande : désactivé par défaut, utilise un modèle local compatible OpenAI.
    pub command_enabled: bool,
    pub shortcut_command: String,
    pub command_url: String,
    pub command_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            url: "http://127.0.0.1:8080".into(),
            model: "large-v3-turbo".into(),
            language: "auto".into(),
            enhance: true,
            shortcut: "Alt+Space".into(),
            mode: "hold".into(),
            paste_method: "paste".into(),
            restore_clipboard: true,
            paste_delay_ms: 300,
            overlay: true,
            overlay_position: "top".into(),
            update_repo: String::new(),
            auto_update_check: true,
            format: FormatRules::default(),
            dictionary: Vec::new(),
            shortcut_paste_last: "Control+Shift+Space".into(),
            shortcut_cancel: "Control+Backspace".into(),
            auto_insert: true,
            mic: String::new(),
            autostart: false,
            show_dock: false,
            sounds: true,
            sound_volume: 50,
            history_enabled: false,
            keep_audio: false,
            command_enabled: false,
            shortcut_command: "Control+Shift+X".into(),
            command_url: "http://127.0.0.1:8000/v1".into(),
            command_model: "Qwen3.5-4B-UD-Q4_K_XL".into(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), Erreur> {
        let bad = |m: &str| Err(Erreur::Config(m.into()));
        if !(self.url.starts_with("http://") || self.url.starts_with("https://")) {
            return bad("l'adresse doit commencer par http:// ou https://");
        }
        if self.model.trim().is_empty()
            || self.language.trim().is_empty()
            || self.shortcut.trim().is_empty()
        {
            return bad("modèle, langue et raccourci sont obligatoires");
        }
        if !["hold", "toggle"].contains(&self.mode.as_str()) {
            return bad("mode inconnu");
        }
        if !["paste", "direct", "copy"].contains(&self.paste_method.as_str()) {
            return bad("méthode de collage inconnue");
        }
        if !["top", "bottom"].contains(&self.overlay_position.as_str()) {
            return bad("position de la pastille inconnue");
        }
        if !self.update_repo.is_empty() && !crate::update::valid_repo(&self.update_repo) {
            return bad("dépôt de mise à jour invalide (attendu : propriétaire/dépôt)");
        }
        if self.sound_volume > 100 {
            return bad("volume des effets sonores entre 0 et 100");
        }
        if self.format.short_text_words > 10 {
            return bad("correction de texte court : 10 mots au plus");
        }
        if self.dictionary.len() > 500 || self.format.custom_rules.len() > 200 {
            return bad("trop de mots ou de règles");
        }
        let (a, b, c, d) = (
            &self.shortcut,
            &self.shortcut_paste_last,
            &self.shortcut_cancel,
            &self.shortcut_command,
        );
        if a == b || a == c || b == c || a == d || b == d || c == d {
            return bad("deux actions ne peuvent pas partager le même raccourci");
        }
        if self.command_enabled
            && (self.command_url.trim().is_empty()
                || !self.command_url.starts_with("http")
                || self.command_model.trim().is_empty()
                || self.shortcut_command.trim().is_empty())
        {
            return bad("mode Commande : adresse http, modèle et raccourci obligatoires");
        }
        if self.paste_delay_ms > 5000 {
            return bad("délai de collage trop long (5000 ms max)");
        }
        Ok(())
    }
}

pub fn load(path: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    match serde_json::from_str::<Config>(&text) {
        Ok(c) if c.validate().is_ok() => c,
        _ => {
            log("config illisible ou invalide : valeurs par défaut, ancien fichier en .bad");
            let _ = std::fs::rename(path, path.with_extension("bad"));
            Config::default()
        }
    }
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), Erreur> {
    cfg.validate()?;
    let io = |e: std::io::Error| Erreur::Config(e.to_string());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(cfg).map_err(|e| Erreur::Config(e.to_string()))?;
    std::fs::write(&tmp, json).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("diction-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn aller_retour() {
        let p = dir("rt").join("c.json");
        let c = Config {
            shortcut: "Control+Super+Space".into(),
            mode: "toggle".into(),
            ..Config::default()
        };
        save(&p, &c).unwrap();
        assert_eq!(load(&p), c);
    }

    #[test]
    fn fichier_absent_donne_les_defauts() {
        assert_eq!(load(&dir("abs").join("nope.json")), Config::default());
    }

    #[test]
    fn fichier_corrompu_est_mis_de_cote() {
        let p = dir("bad").join("c.json");
        std::fs::write(&p, "{ pas du json").unwrap();
        assert_eq!(load(&p), Config::default());
        assert!(p.with_extension("bad").exists());
    }

    #[test]
    fn champs_manquants_prennent_le_defaut() {
        let p = dir("part").join("c.json");
        std::fs::write(&p, r#"{"mode":"toggle"}"#).unwrap();
        let c = load(&p);
        assert_eq!(c.mode, "toggle");
        assert_eq!(c.url, Config::default().url);
    }

    #[test]
    fn validation_refuse_les_valeurs_incorrectes() {
        for c in [
            Config {
                url: "ftp://x".into(),
                ..Config::default()
            },
            Config {
                mode: "x".into(),
                ..Config::default()
            },
            Config {
                paste_method: "x".into(),
                ..Config::default()
            },
            Config {
                overlay_position: "x".into(),
                ..Config::default()
            },
            Config {
                paste_delay_ms: 9999,
                ..Config::default()
            },
            Config {
                update_repo: "pas un depot".into(),
                ..Config::default()
            },
            Config {
                shortcut: " ".into(),
                ..Config::default()
            },
        ] {
            assert!(matches!(c.validate(), Err(Erreur::Config(_))));
        }
        assert!(Config::default().validate().is_ok());
    }
}
