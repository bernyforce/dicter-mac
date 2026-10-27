//! Vérification des mises à jour par les « releases » GitHub. Lecture seule : rien n'est installé tout seul.

use std::time::Duration;

use crate::erreur::Erreur;

#[derive(Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub url: String,
}

/// « propriétaire/dépôt » : lettres, chiffres, point, tiret, souligné. Évite d'injecter autre chose dans l'URL.
pub fn valid_repo(r: &str) -> bool {
    let ok = |s: &str| {
        !s.is_empty()
            && !s.chars().all(|c| c == '.')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    matches!(r.split_once('/'), Some((o, n)) if ok(o) && ok(n))
}

/// « v1.2.3 », « 1.2.3-beta+4 » -> (1, 2, 3). Les suffixes de pré-version sont ignorés.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

pub fn is_newer(current: &str, tag: &str) -> bool {
    matches!((parse_version(current), parse_version(tag)), (Some(c), Some(t)) if t > c)
}

pub fn parse_release(json: &str) -> Result<Release, Erreur> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| Erreur::Reponse(e.to_string()))?;
    match (v["tag_name"].as_str(), v["html_url"].as_str()) {
        (Some(tag), Some(url)) => Ok(Release {
            tag: tag.into(),
            url: url.into(),
        }),
        _ => Err(Erreur::Reponse("release sans tag_name ou html_url".into())),
    }
}

pub fn latest(repo: &str) -> Result<Release, Erreur> {
    if !valid_repo(repo) {
        return Err(Erreur::Config(
            "dépôt de mise à jour invalide (attendu : propriétaire/dépôt)".into(),
        ));
    }
    let r = ureq::get(&format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ))
    .set("Accept", "application/vnd.github+json")
    .set("User-Agent", "diction-mac")
    .timeout(Duration::from_secs(8))
    .call();
    match r {
        Ok(resp) => parse_release(
            &resp
                .into_string()
                .map_err(|e| Erreur::Reponse(e.to_string()))?,
        ),
        Err(ureq::Error::Status(404, _)) => {
            Err(Erreur::Update("aucune release publiée sur ce dépôt".into()))
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.3.0-beta+7"), Some((0, 3, 0)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("x.y.z"), None);
    }

    #[test]
    fn comparaison() {
        assert!(is_newer("0.3.0", "v0.3.1"));
        assert!(is_newer("0.9.9", "v0.10.0"));
        assert!(!is_newer("0.3.0", "v0.3.0"));
        assert!(!is_newer("1.0.0", "v0.9.0"));
        assert!(!is_newer("0.3.0", "n'importe quoi"));
    }

    #[test]
    fn depot() {
        assert!(valid_repo("bernyforce/diction-mac"));
        for bad in ["", "a", "a/b/c", "a/ b", "../x", "a/b?x=1", "/b"] {
            assert!(!valid_repo(bad), "{bad}");
        }
    }

    #[test]
    fn release_json() {
        let r = parse_release(r#"{"tag_name":"v0.4.0","html_url":"https://github.com/o/r/releases/tag/v0.4.0","x":1}"#).unwrap();
        assert_eq!(r.tag, "v0.4.0");
        assert!(matches!(parse_release("{}"), Err(Erreur::Reponse(_))));
    }
}
