//! Dernières transcriptions : en mémoire seulement, sauf si l'utilisateur active la persistance.

use std::path::Path;

pub const MAX: usize = 5;

#[derive(Default)]
pub struct History(pub Vec<String>);

impl History {
    pub fn push(&mut self, text: &str) {
        self.0.insert(0, text.to_string());
        self.0.truncate(MAX);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn last(&self) -> Option<&String> {
        self.0.first()
    }

    pub fn load(path: &Path) -> History {
        let items = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default();
        History(items.into_iter().take(MAX).collect())
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(
            path,
            serde_json::to_string(&self.0).unwrap_or_else(|_| "[]".into()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garde_les_cinq_dernieres_la_plus_recente_d_abord() {
        let mut h = History::default();
        for i in 0..7 {
            h.push(&format!("t{i}"));
        }
        assert_eq!(h.0.len(), MAX);
        assert_eq!(h.last().unwrap(), "t6");
    }

    #[test]
    fn persistance_aller_retour() {
        let p = std::env::temp_dir().join(format!("diction-hist-{}.json", std::process::id()));
        let mut h = History::default();
        h.push("a");
        h.save(&p).unwrap();
        assert_eq!(History::load(&p).0, vec!["a".to_string()]);
        assert!(History::load(&p.with_extension("nope")).0.is_empty());
    }
}
