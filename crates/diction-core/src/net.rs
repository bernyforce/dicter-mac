//! Appels à la passerelle Diction (API de transcription compatible OpenAI).

use std::time::{Duration, Instant};

use crate::config::Config;
use crate::erreur::Erreur;

pub fn extract_text(json: &str) -> Result<String, Erreur> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| Erreur::Reponse(e.to_string()))?;
    v["text"]
        .as_str()
        .map(|t| t.trim().to_string())
        .ok_or_else(|| Erreur::Reponse("champ \"text\" absent".into()))
}

fn field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    let value = value.replace(['\r', '\n'], " ");
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .as_bytes(),
    );
}

pub fn transcribe(cfg: &Config, filename: &str, audio: &[u8]) -> Result<String, Erreur> {
    let boundary = "----dictionmac";
    let mut body = Vec::new();
    field(&mut body, boundary, "model", &cfg.model);
    if cfg.language != "auto" {
        field(&mut body, boundary, "language", &cfg.language);
    }
    if !cfg.dictionary.is_empty() {
        field(&mut body, boundary, "prompt", &cfg.dictionary.join(", "));
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(audio);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let mut url = format!("{}/v1/audio/transcriptions", cfg.url.trim_end_matches('/'));
    if cfg.enhance {
        url.push_str("?enhance=true");
    }
    let resp = ureq::post(&url)
        .timeout(Duration::from_secs(60))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)?;
    extract_text(
        &resp
            .into_string()
            .map_err(|e| Erreur::Reponse(e.to_string()))?,
    )
}

/// Liste des modèles annoncés par la passerelle et temps de réponse en ms.
pub fn models(url: &str) -> Result<(Vec<String>, u128), Erreur> {
    let t = Instant::now();
    let resp = ureq::get(&format!("{}/v1/models", url.trim_end_matches('/')))
        .timeout(Duration::from_secs(5))
        .call()?;
    let ms = t.elapsed().as_millis();
    let v: serde_json::Value = serde_json::from_str(
        &resp
            .into_string()
            .map_err(|e| Erreur::Reponse(e.to_string()))?,
    )
    .map_err(|e| Erreur::Reponse(e.to_string()))?;
    let ids = v["data"]
        .as_array()
        .ok_or_else(|| Erreur::Reponse("champ \"data\" absent".into()))?
        .iter()
        .filter_map(|m| m["id"].as_str().map(String::from))
        .collect();
    Ok((ids, ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrait_le_texte() {
        assert_eq!(
            extract_text(r#"{"mode":"transcribe","text":" Bonjour. "}"#),
            Ok("Bonjour.".into())
        );
    }

    #[test]
    fn json_invalide_donne_une_erreur() {
        assert!(matches!(
            extract_text("pas du json"),
            Err(Erreur::Reponse(_))
        ));
        assert!(matches!(extract_text("{}"), Err(Erreur::Reponse(_))));
    }

    #[test]
    fn passerelle_hors_ligne_est_signalee() {
        let cfg = Config {
            url: "http://127.0.0.1:1".into(),
            ..Config::default()
        };
        assert!(matches!(
            transcribe(&cfg, "x", b"x"),
            Err(Erreur::PasserelleHorsLigne(_))
        ));
        assert!(matches!(
            models("http://127.0.0.1:1"),
            Err(Erreur::PasserelleHorsLigne(_))
        ));
    }
}
