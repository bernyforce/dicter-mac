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

/// Version courante du format. 0 = fichier d'avant le versionnage (0.4.1 et avant).
pub const CONFIG_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Version du format du fichier. Les anciens fichiers n'en ont pas : `migrate()` les met à niveau.
    pub version: u32,
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
    /// Taille de la pastille : "small", "medium" ou "large".
    pub overlay_size: String,
    /// Copie aussi le texte dans le presse-papiers après l'insertion.
    pub copy_to_clipboard: bool,
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
    pub sound_start: bool,
    pub sound_stop: bool,
    pub sound_volume: u8,
    /// Niveau du journal : "error", "info" ou "debug".
    pub log_level: String,
    pub history_enabled: bool,
    pub keep_audio: bool,
    /// Mode Commande : désactivé par défaut, utilise un modèle local compatible OpenAI.
    pub command_enabled: bool,
    pub shortcut_command: String,
    pub command_url: String,
    pub command_model: String,
    /// Remplace les retours à la ligne par des espaces avant le collage.
    pub single_line: bool,
    /// Baisse le volume système pendant l'enregistrement (restauré à la fin).
    pub duck_others: bool,
    /// Volume pendant l'enregistrement, en % du volume actuel (0 à 80).
    pub duck_percent: u8,
    /// Affiche le texte reconnu dans la pastille avant de la masquer.
    pub show_result: bool,
    /// Durée d'affichage du résultat, en secondes (1 à 10).
    pub result_seconds: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_VERSION,
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
            overlay_size: "medium".into(),
            copy_to_clipboard: false,
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
            sound_start: true,
            sound_stop: true,
            sound_volume: 50,
            log_level: "info".into(),
            history_enabled: false,
            keep_audio: false,
            command_enabled: false,
            shortcut_command: "Control+Shift+X".into(),
            command_url: "http://127.0.0.1:8000/v1".into(),
            command_model: "Qwen3.5-4B-UD-Q4_K_XL".into(),
            single_line: false,
            duck_others: false,
            duck_percent: 20,
            show_result: false,
            result_seconds: 3,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), Erreur> {
        let bad = |m: &str| Err(Erreur::Config(m.into()));
        if !has_http_scheme(&self.url) {
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
        if !["paste", "direct", "copy", "terminal"].contains(&self.paste_method.as_str()) {
            return bad("méthode de collage inconnue");
        }
        if !["top", "bottom"].contains(&self.overlay_position.as_str()) {
            return bad("position de la pastille inconnue");
        }
        if !["small", "medium", "large"].contains(&self.overlay_size.as_str()) {
            return bad("taille de la pastille inconnue");
        }
        if !self.update_repo.is_empty() && !crate::update::valid_repo(&self.update_repo) {
            return bad("dépôt de mise à jour invalide (attendu : propriétaire/dépôt)");
        }
        if !["error", "info", "debug"].contains(&self.log_level.as_str()) {
            return bad("niveau de journal inconnu");
        }
        if self.sound_volume > 100 {
            return bad("volume des effets sonores entre 0 et 100");
        }
        if self.duck_percent > 80 {
            return bad("volume pendant l'enregistrement : 80 % au plus");
        }
        if !(1..=10).contains(&self.result_seconds) {
            return bad("durée d'affichage du résultat entre 1 et 10 secondes");
        }
        if self.format.short_text_words > 10 {
            return bad("correction de texte court : 10 mots au plus");
        }
        if self.dictionary.len() > 500 || self.format.custom_rules.len() > 200 {
            return bad("trop de mots ou de règles");
        }
        // Un raccourci vide (« Effacer ») est permis sauf pour l'enregistrement ; les vides ne comptent pas comme doublons.
        let sc = [
            &self.shortcut,
            &self.shortcut_paste_last,
            &self.shortcut_cancel,
            &self.shortcut_command,
        ];
        for (i, a) in sc.iter().enumerate() {
            if !a.trim().is_empty() && sc[i + 1..].contains(a) {
                return bad("deux actions ne peuvent pas partager le même raccourci");
            }
        }
        if self.command_enabled
            && (self.command_url.trim().is_empty()
                || !has_http_scheme(&self.command_url)
                || self.command_model.trim().is_empty()
                || self.shortcut_command.trim().is_empty())
        {
            return bad(
                "mode Commande : adresse http:// ou https://, modèle et raccourci obligatoires",
            );
        }
        if self.paste_delay_ms > 5000 {
            return bad("délai de collage trop long (5000 ms max)");
        }
        Ok(())
    }
}

/// Met à niveau un JSON de réglages d'une ancienne version. Pure : ne touche ni au disque ni au journal.
pub fn migrate(mut v: serde_json::Value) -> serde_json::Value {
    let Some(o) = v.as_object_mut() else { return v };
    let from = o.get("version").and_then(|x| x.as_u64()).unwrap_or(0);
    if from < 1 {
        // 0 -> 1 : l'interrupteur unique `sounds` devient `sound_start` et `sound_stop`.
        if let Some(old) = o.remove("sounds") {
            o.entry("sound_start").or_insert(old.clone());
            o.entry("sound_stop").or_insert(old);
        }
    }
    // 1 -> 2 : champs duck_* et show_result ajoutés, leurs valeurs par défaut viennent de serde.
    o.insert("version".into(), CONFIG_VERSION.into());
    v
}

impl Config {
    /// Facteur d'échelle de la pastille (medium = taille d'origine).
    pub fn overlay_scale(&self) -> f64 {
        match self.overlay_size.as_str() {
            "small" => 0.8,
            "large" => 1.3,
            _ => 1.0,
        }
    }

    /// Lit un JSON de réglages de n'importe quelle version.
    pub fn from_json(text: &str) -> Result<Config, serde_json::Error> {
        serde_json::from_value(migrate(serde_json::from_str(text)?))
    }
}

fn has_http_scheme(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Vrai si l'hôte de l'adresse est cette machine ou un réseau privé (192.168.x, 10.x, Tailscale 100.64-127.x).
pub fn host_is_local(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    let auth = auth.rsplit_once('@').map_or(auth, |(_, h)| h);
    let host = match auth.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => auth.split(':').next().unwrap_or(""),
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let o = ip.octets();
            ip.is_loopback()
                || o[0] == 10
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 100 && (64..=127).contains(&o[1]))
        }
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Hôte (sans schéma ni port) pour comparer deux adresses.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    let auth = auth.rsplit_once('@').map_or(auth, |(_, h)| h);
    match auth.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => auth.split(':').next().unwrap_or(""),
    }
    .to_ascii_lowercase()
}

pub fn load(path: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    match Config::from_json(&text) {
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
                url: "httpx://x".into(),
                ..Config::default()
            },
            Config {
                command_enabled: true,
                command_url: "httpx://x".into(),
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
            Config {
                duck_percent: 81,
                ..Config::default()
            },
            Config {
                result_seconds: 0,
                ..Config::default()
            },
            Config {
                result_seconds: 11,
                ..Config::default()
            },
        ] {
            assert!(matches!(c.validate(), Err(Erreur::Config(_))));
        }
        assert!(Config::default().validate().is_ok());
    }

    // JSON figés d'anciennes versions : ils doivent rester lisibles.
    const V041_SONS_COUPES: &str =
        r#"{"url":"http://127.0.0.1:8080","mode":"toggle","sounds":false,"sound_volume":30}"#;
    const V050: &str = r#"{"version":1,"mode":"toggle","sound_volume":30,"single_line":true}"#;
    const V030: &str = r#"{"mode":"hold","shortcut":"Alt+Space","overlay":true}"#;

    #[test]
    fn les_bornes_du_duck_et_du_resultat_sont_acceptees() {
        for (p, s) in [(0, 1), (80, 10)] {
            let c = Config {
                duck_percent: p,
                result_seconds: s,
                ..Config::default()
            };
            assert!(c.validate().is_ok());
        }
    }

    #[test]
    fn migration_v050_donne_les_nouveaux_reglages_a_faux() {
        let c = Config::from_json(V050).unwrap();
        assert!(!c.duck_others && !c.show_result);
        assert_eq!((c.duck_percent, c.result_seconds), (20, 3));
        assert!(c.single_line && c.validate().is_ok());
        assert_eq!(c.version, CONFIG_VERSION);
    }

    #[test]
    fn migration_sounds_faux_donne_les_deux_sons_coupes() {
        let c = Config::from_json(V041_SONS_COUPES).unwrap();
        assert!(!c.sound_start && !c.sound_stop);
        assert_eq!((c.sound_volume, c.mode.as_str()), (30, "toggle"));
        assert_eq!(c.version, CONFIG_VERSION);
    }

    #[test]
    fn migration_sans_sounds_garde_les_defauts_et_la_v030_se_lit() {
        let c = Config::from_json(V030).unwrap();
        assert!(c.sound_start && c.sound_stop && c.validate().is_ok());
        assert_eq!(c.version, CONFIG_VERSION);
    }

    #[test]
    fn migration_est_idempotente_et_respecte_les_nouveaux_champs() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"sounds":true,"sound_start":false}"#).unwrap();
        let m = migrate(v);
        assert_eq!(m["sound_start"], false);
        assert_eq!(m["sound_stop"], true);
        assert!(m.get("sounds").is_none());
        assert_eq!(migrate(m.clone()), m);
    }

    #[test]
    fn migrate_recopie_sounds_sur_les_deux_champs() {
        for b in [true, false] {
            let m = migrate(serde_json::json!({ "sounds": b }));
            assert_eq!(m["sound_start"], serde_json::json!(b));
            assert_eq!(m["sound_stop"], serde_json::json!(b));
            assert!(m.get("sounds").is_none());
        }
    }

    #[test]
    fn nouveaux_reglages_lot5b() {
        let d = Config::default();
        assert_eq!(
            (d.overlay_size.as_str(), d.copy_to_clipboard),
            ("medium", false)
        );
        assert_eq!(d.overlay_scale(), 1.0);
        assert!(
            Config {
                paste_method: "terminal".into(),
                ..d.clone()
            }
            .validate()
            .is_ok()
        );
        assert!(
            Config {
                overlay_size: "xl".into(),
                ..d.clone()
            }
            .validate()
            .is_err()
        );
        // un ancien fichier reste lisible
        assert_eq!(Config::from_json(V030).unwrap().overlay_size, "medium");
        // raccourcis effacés : permis (sauf l'enregistrement), les vides ne sont pas des doublons
        let vides = Config {
            shortcut_paste_last: String::new(),
            shortcut_cancel: String::new(),
            ..d.clone()
        };
        assert!(vides.validate().is_ok());
        assert!(
            Config {
                shortcut: String::new(),
                ..d
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn niveau_de_journal_inconnu_refuse() {
        let c = Config {
            log_level: "x".into(),
            ..Config::default()
        };
        assert!(matches!(c.validate(), Err(Erreur::Config(_))));
    }

    /// Chaque champ de Config doit avoir son réglage dans l'interface, et chaque réglage son champ.
    #[test]
    fn parite_interface_et_config() {
        fn feuilles(v: &serde_json::Value, pre: &str, out: &mut Vec<String>) {
            match v.as_object() {
                Some(o) => o.iter().for_each(|(k, x)| {
                    let p = if pre.is_empty() {
                        k.clone()
                    } else {
                        format!("{pre}.{k}")
                    };
                    feuilles(x, &p, out)
                }),
                None => out.push(pre.to_string()),
            }
        }
        let ui = include_str!("../../../ui/index.html");
        let attr = |name: &str| -> Vec<String> {
            let pat = format!("{name}=\"");
            ui.match_indices(&pat)
                .filter_map(|(i, _)| {
                    let r = &ui[i + pat.len()..];
                    r.split('"').next().map(String::from)
                })
                .collect()
        };
        let (k, sc) = (attr("data-k"), attr("data-sc"));
        let mut champs = Vec::new();
        feuilles(
            &serde_json::to_value(Config::default()).unwrap(),
            "",
            &mut champs,
        );
        // Seule exception : `version`, gérée par le code et jamais par l'utilisateur.
        for c in champs.iter().filter(|c| *c != "version") {
            assert!(k.contains(c) || sc.contains(c), "pas de réglage pour {c}");
        }
        for r in k.iter().chain(sc.iter()) {
            assert!(champs.contains(r), "réglage sans champ : {r}");
        }
    }

    #[test]
    fn hotes_locaux() {
        for u in [
            "http://127.0.0.1:8080",
            "http://localhost:8000/v1",
            "http://[::1]:8080",
            "http://192.168.1.5",
            "http://10.0.0.2:80/x",
            "http://100.64.0.1",
            "http://100.127.255.1",
        ] {
            assert!(host_is_local(u), "{u}");
        }
        for u in [
            "https://exemple.com",
            "http://localhost.evil.com",
            "http://127.0.0.1@evil.com/",
            "http://100.63.0.1",
            "http://100.128.0.1",
            "http://172.16.0.1",
            "http://8.8.8.8",
        ] {
            assert!(!host_is_local(u), "{u}");
        }
    }

    #[test]
    fn hote_extrait() {
        assert_eq!(host_of("http://User@Exemple.com:80/a"), "exemple.com");
        assert_eq!(host_of("http://[::1]:8080"), "::1");
    }
}
