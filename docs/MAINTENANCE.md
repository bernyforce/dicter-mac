# Protocole de maintenance

## Structure
| Module | Rôle | Dépendances directes |
|---|---|---|
| `diction-core` | réglages, erreurs, client de la passerelle, mises à jour | serde, serde_json, ureq |
| `diction-audio` | micro et WAV | cpal |
| `diction-platform` | collage, notifications, autorisations (seul module à adapter pour Windows et Linux) | aucune |
| `diction-mac` | coque Tauri : fenêtres, barre de menus, raccourci | tauri, tauri-plugin-global-shortcut |

Règle : `diction-core` ne dépend d'aucune bibliothèque graphique ni système. Toute nouvelle dépendance doit être justifiée dans le commit.

## Versions et journal des changements
- Numérotation SemVer ; tant que la version commence par 0, aucune compatibilité n'est promise.
- Chaque changement visible entre dans `CHANGELOG.md` sous Added, Changed, Deprecated, Removed, Fixed ou Security.
- Une release = une étiquette `vX.Y.Z` posée sur le commit de la version du `Cargo.toml` et de `tauri.conf.json`.

## Avant chaque commit
```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Publier une release
1. Mettre à jour la version (`Cargo.toml` de l'espace de travail et `src-tauri/tauri.conf.json`) et `CHANGELOG.md`.
2. `git tag vX.Y.Z && git push --tags` : le workflow `release.yml` compile l'app et joint l'archive à la release GitHub.
3. L'app voit la release avec « Vérifier maintenant » (réglage « Mises à jour », dépôt `propriétaire/dépôt`).
L'app ne télécharge ni n'installe rien toute seule : elle ouvre la page de la release.

## Pourquoi pas le plugin de mise à jour de Tauri ?
Il installe automatiquement, mais impose une clé de signature privée (perdre la clé interdit toute mise à jour future) et des fichiers `latest.json` signés. Ce coût n'est pas justifié tant qu'il y a un seul utilisateur. À reconsidérer si l'app est distribuée. Source : documentation Tauri v2, page « Updater ».

## Diagnostiquer une panne
1. Réglages > Journal > « Exporter le diagnostic » : crée `~/Library/Logs/DictionMac-diagnostic.txt` (versions, réglages, fin du journal ; jamais le texte dicté).
2. Chercher le code dans le journal `~/Library/Logs/DictionMac.log`.

| Code | Étape | Première vérification |
|---|---|---|
| E-MIC | ouverture du micro | autorisation Microphone, périphérique d'entrée |
| E-NET-OFF | passerelle injoignable | passerelle lancée ? adresse ? Tailscale ? |
| E-NET-HTTP | refus de la passerelle | code HTTP dans le message |
| E-NET-RESP | réponse non lisible | version de la passerelle |
| E-PASTE | collage (⌘V, Terminal ⌃⇧V, direct) | autorisation Accessibilité ; le texte reste dans le presse-papiers |
| E-CFG | réglage invalide | valeur refusée citée dans le message |
| E-UPD | mise à jour | dépôt `propriétaire/dépôt`, accès à api.github.com |
| E-DUCK | volume système (osascript) | sans gravité, la dictée continue ; vérifier `osascript` et la sortie audio |
| E-PANIC | bogue du programme | lieu (`fichier:ligne`) dans le message |
| E-START | démarrage impossible | message de Tauri dans le journal |

Ajouter un code : une variante de `Erreur` dans `diction-core/src/erreur.rs`, un cas dans `code()` et `Display`, une ligne dans ce tableau.

## Recette manuelle (à refaire avant chaque release)
Les autorisations macOS empêchent de les automatiser.
1. Lancer l'app : la fenêtre s'ouvre. La relancer : la fenêtre se rouvre, un seul processus.
2. Autorisations : « Tester le micro » est vert, Accessibilité est verte.
3. Dicter avec le raccourci dans un éditeur : le texte est collé, la pastille s'affiche et oscille avec la voix.
4. Couper la passerelle : la pastille affiche `[E-NET-OFF]`.
5. Changer le raccourci : l'ancien ne répond plus, le nouveau marche.

## Sécurité
- Le marqueur de presse-papiers confidentiel n'est pas posé : `pbcopy` ne permet pas de le définir (limite connue). Le texte dicté reste donc visible dans les gestionnaires de presse-papiers pendant le collage.
- Aucun secret dans le dépôt ni dans le journal. Le texte dicté n'est jamais écrit sur disque (sauf historique, désactivé par défaut).
- Les adresses venant du réseau ne sont ouvertes que si elles commencent par `https://github.com/`.
- Le dépôt de mise à jour est validé (`propriétaire/dépôt`, caractères sûrs) avant de construire l'URL.
- Audit des dépendances : `cargo audit` (base RustSec) dans le workflow CI.

## Réglages ajoutés en 0.5.0
- `paste_method` : `paste`, `direct`, `copy` ou `terminal` (⌃⇧V). `copy_to_clipboard` : garde le texte dans le presse-papiers (pas de restauration).
- `format.custom_rules` : ligne `[exact] a => b` ou `[smart] a => b`, sinon sous-chaîne. Pas de regex. Une règle sans `=>` ou sans motif est ignorée et écrite au journal.
- `overlay_size` : `small` (0,8), `medium` (1), `large` (1,3).
- Raccourcis vides permis sauf `shortcut`. La version du fichier reste 1 (nouveaux champs avec valeur par défaut).

## Réglages ajoutés en 0.6.0
- `duck_others` (faux), `duck_percent` (20, de 0 à 80, % du volume actuel), `show_result` (faux), `result_seconds` (3, de 1 à 10). La version du fichier passe à 2.
- Le volume est lu et posé par `osascript` (`diction-platform/src/duck.rs`). `restore` est idempotent et appelé dans `stop_recording`, `cancel_recording`, `finish`, le hook de panique, l'arrêt de l'app et au lancement. Filet de sécurité : `duck-pending` (0600) dans le dossier de configuration, supprimé à la restauration.
- Le texte reconnu va à la pastille par l'événement `result`, jamais au journal.
