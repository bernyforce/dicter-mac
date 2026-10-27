//! Règles de formatage du texte transcrit. Fonctions pures, sans réseau ni système.

use crate::config::FormatRules;

const HESITATIONS: [&str; 8] = ["euh", "euhh", "heu", "hum", "hmm", "um", "uh", "uhm"];

fn is_hesitation(token: &str) -> bool {
    let w = token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    HESITATIONS.contains(&w.as_str())
}

/// Retire « euh », « um », « hmm »… et la ponctuation qui les accompagne. Remet la majuscule initiale si besoin.
fn remove_hesitations(text: &str) -> String {
    let had_cap = text.chars().next().is_some_and(char::is_uppercase);
    let kept: Vec<&str> = text
        .split_whitespace()
        .filter(|t| !is_hesitation(t))
        .collect();
    let mut out = kept.join(" ");
    if had_cap && let Some(first) = out.chars().next() {
        out = first.to_uppercase().collect::<String>() + &out[first.len_utf8()..];
    }
    out
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

pub fn apply(text: &str, rules: &FormatRules) -> String {
    let mut t = text.trim().to_string();
    if rules.remove_hesitations {
        t = remove_hesitations(&t);
    }
    for rule in &rules.custom_rules {
        if let Some((from, to)) = rule.split_once("=>") {
            let from = from.trim();
            if !from.is_empty() {
                t = t.replace(from, to.trim());
            }
        }
    }
    if rules.short_text_words > 0 && word_count(&t) <= rules.short_text_words as usize {
        t = t
            .to_lowercase()
            .trim_matches(|c: char| ".,;:!?".contains(c))
            .to_string();
    }
    if rules.space_before_qe {
        t = t
            .replace(" ?", "?")
            .replace(" !", "!")
            .replace('?', " ?")
            .replace('!', " !");
    }
    if rules.trailing_space && !t.is_empty() {
        t.push(' ');
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> FormatRules {
        FormatRules::default()
    }

    #[test]
    fn sans_regle_le_texte_est_inchange() {
        assert_eq!(apply("  Bonjour le monde.  ", &r()), "Bonjour le monde.");
    }

    #[test]
    fn hesitations() {
        let rules = FormatRules {
            remove_hesitations: true,
            ..r()
        };
        assert_eq!(
            apply("Euh, je voudrais um réserver.", &rules),
            "Je voudrais réserver."
        );
        assert_eq!(apply("Humain", &rules), "Humain");
    }

    #[test]
    fn texte_court() {
        let rules = FormatRules {
            short_text_words: 3,
            ..r()
        };
        assert_eq!(apply("Bonjour.", &rules), "bonjour");
        assert_eq!(
            apply("Je voudrais réserver une table.", &rules),
            "Je voudrais réserver une table."
        );
    }

    #[test]
    fn espace_avant_points_et_espace_final() {
        let rules = FormatRules {
            space_before_qe: true,
            trailing_space: true,
            ..r()
        };
        assert_eq!(apply("Vraiment? Oui !", &rules), "Vraiment ? Oui ! ");
    }

    #[test]
    fn regles_personnalisees() {
        let rules = FormatRules {
            custom_rules: vec!["ia => IA".into(), "pas de flèche".into(), " => x".into()],
            ..r()
        };
        assert_eq!(apply("une ia utile", &rules), "une IA utile");
    }
}
