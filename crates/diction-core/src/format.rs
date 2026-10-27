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

/// Mode de correspondance d'une règle : préfixe `[exact] ` ou `[smart] `, sinon sous-chaîne (comportement d'origine).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Match {
    Substring,
    /// Mot entier, sensible à la casse, entouré d'espaces ou du bord du texte.
    Exact,
    /// Mot entier, insensible à la casse, la ponctuation voisine est ignorée.
    Smart,
}

/// « [mode] chercher => remplacer ». None si la règle est invalide (sans `=>` ou sans motif).
pub fn parse_rule(line: &str) -> Option<(Match, &str, &str)> {
    let (mode, body) = if let Some(r) = line.strip_prefix("[exact]") {
        (Match::Exact, r)
    } else if let Some(r) = line.strip_prefix("[smart]") {
        (Match::Smart, r)
    } else {
        (Match::Substring, line)
    };
    let (from, to) = body.split_once("=>")?;
    let from = from.trim();
    (!from.is_empty()).then_some((mode, from, to.trim()))
}

fn replace_word(text: &str, from: &str, to: &str, mode: Match) -> String {
    let (t, f): (Vec<char>, Vec<char>) = (text.chars().collect(), from.chars().collect());
    let same = |a: char, b: char| match mode {
        Match::Smart => a.to_lowercase().eq(b.to_lowercase()),
        _ => a == b,
    };
    let edge = |c: Option<&char>| match (mode, c) {
        (_, None) => true,
        (Match::Exact, Some(c)) => c.is_whitespace(),
        (_, Some(c)) => !c.is_alphanumeric(),
    };
    let mut out = String::new();
    let mut i = 0;
    while i < t.len() {
        let end = i + f.len();
        let hit = end <= t.len()
            && t[i..end].iter().zip(&f).all(|(a, b)| same(*a, *b))
            && edge(i.checked_sub(1).and_then(|j| t.get(j)))
            && edge(t.get(end));
        if hit {
            out.push_str(to);
            i = end;
        } else {
            out.push(t[i]);
            i += 1;
        }
    }
    out
}

pub fn apply(text: &str, rules: &FormatRules) -> String {
    let mut t = text.trim().to_string();
    if rules.remove_hesitations {
        t = remove_hesitations(&t);
    }
    for rule in &rules.custom_rules {
        match parse_rule(rule) {
            Some((Match::Substring, from, to)) => t = t.replace(from, to),
            Some((mode, from, to)) => t = replace_word(&t, from, to, mode),
            None if !rule.trim().is_empty() => {
                crate::erreur::log(&format!("règle personnalisée ignorée : {rule}"))
            }
            None => {}
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

    fn avec(rule: &str, text: &str) -> String {
        let rules = FormatRules {
            custom_rules: vec![rule.into()],
            ..r()
        };
        apply(text, &rules)
    }

    #[test]
    fn mode_defaut_inchange_sous_chaine() {
        assert_eq!(avec("ia => IA", "bia et ia"), "bIA et IA");
        assert_eq!(avec("ia => IA", "Ia"), "Ia");
    }

    #[test]
    fn mode_exact() {
        assert_eq!(avec("[exact] ia => IA", "une ia utile"), "une IA utile");
        assert_eq!(avec("[exact] ia => IA", "bia Ia ia."), "bia Ia ia.");
    }

    #[test]
    fn mode_smart() {
        assert_eq!(avec("[smart] ia => IA", "Ia, l'ia."), "IA, l'IA.");
        assert_eq!(avec("[smart] ia => IA", "bia piano"), "bia piano");
    }

    #[test]
    fn regles_de_modes_differents_dans_l_ordre() {
        let rules = FormatRules {
            custom_rules: vec!["[exact] ia => IA".into(), "IA => intelligence".into()],
            ..r()
        };
        assert_eq!(apply("une ia", &rules), "une intelligence");
    }

    #[test]
    fn regle_invalide_ignoree_sans_panique() {
        assert_eq!(avec("[exact] sans fleche", "texte"), "texte");
        assert_eq!(avec("[smart] => x", "texte"), "texte");
    }
}
