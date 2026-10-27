//! Mode Commande : une instruction dictée transforme le texte sélectionné (modèle local, API OpenAI).

use std::time::Duration;

use crate::config::Config;
use crate::erreur::Erreur;

const SYSTEM: &str = "Tu appliques l'instruction de l'utilisateur au texte fourni. \
Réponds uniquement avec le texte résultat, sans commentaire ni guillemets. \
Sans texte fourni, réponds brièvement à la question.";
const MAX_REPONSE: usize = 20_000;

pub fn request_body(cfg: &Config, instruction: &str, selection: &str) -> String {
    let user = if selection.is_empty() {
        instruction.to_string()
    } else {
        format!("Instruction : {instruction}\n\nTexte :\n{selection}")
    };
    serde_json::json!({
        "model": cfg.command_model,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": user},
        ],
    })
    .to_string()
}

pub fn extract_reply(json: &str) -> Result<String, Erreur> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| Erreur::Llm(format!("réponse illisible : {e}")))?;
    let text = v["choices"][0]["message"]["content"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| Erreur::Llm("réponse vide".into()))?;
    if text.chars().count() > MAX_REPONSE {
        return Err(Erreur::Llm("réponse trop longue".into()));
    }
    Ok(text.to_string())
}

/// Le texte d'origine n'est jamais touché ici : en cas d'échec l'appelant ne colle rien.
pub fn run(cfg: &Config, instruction: &str, selection: &str) -> Result<String, Erreur> {
    let url = format!("{}/chat/completions", cfg.command_url.trim_end_matches('/'));
    let body = request_body(cfg, instruction, selection);
    let resp = ureq::post(&url)
        .timeout(Duration::from_secs(30))
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|e| match e {
            ureq::Error::Status(c, _) => Erreur::Llm(format!("HTTP {c}")),
            ureq::Error::Transport(t) => Erreur::Llm(format!("modèle injoignable : {t}")),
        })?;
    extract_reply(&resp.into_string().map_err(|e| Erreur::Llm(e.to_string()))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corps_avec_et_sans_selection() {
        let cfg = Config::default();
        let b: serde_json::Value =
            serde_json::from_str(&request_body(&cfg, "traduis", "Salut")).unwrap();
        assert!(
            b["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("Salut")
        );
        let b: serde_json::Value =
            serde_json::from_str(&request_body(&cfg, "quelle heure", "")).unwrap();
        assert_eq!(b["messages"][1]["content"], "quelle heure");
    }

    #[test]
    fn reponse_valide_vide_ou_cassee() {
        assert_eq!(
            extract_reply(r#"{"choices":[{"message":{"content":" Hello "}}]}"#).unwrap(),
            "Hello"
        );
        assert!(extract_reply(r#"{"choices":[{"message":{"content":"  "}}]}"#).is_err());
        assert!(extract_reply("pas du json").is_err());
        assert!(extract_reply("{}").is_err());
    }
}
