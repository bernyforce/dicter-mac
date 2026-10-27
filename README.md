# DICTER pour Mac

Dictée vocale pour Mac. Une pastille s'affiche pendant l'enregistrement, le texte nettoyé est collé dans l'app active.
Les réglages (passerelle, modèle, raccourci, collage, pastille) se changent dans la fenêtre « DICTER », ouverte depuis l'icône de la barre de menus.

## Construire et lancer
```
cargo install tauri-cli --version "^2" --locked   # une seule fois
cd src-tauri && cargo tauri build --bundles app
open ../target/release/bundle/macos/DICTER.app
```
Autorisations macOS : Microphone (le bouton « Tester le micro » la demande) et Accessibilité (pour coller). La fenêtre « Autorisations » les vérifie.

## Raccourci
Par défaut ⌥ Espace en mode « maintenir pour parler ». Le raccourci se change en cliquant dessus puis en appuyant la combinaison.
Limite : une combinaison de modificateurs seuls (par exemple Ctrl+Commande sans autre touche) n'est pas possible avec ce mécanisme. Évite aussi ⌃⌘Espace, réservé par macOS au sélecteur d'emojis.

## Vérifier
```
cargo test --workspace
./target/release/bundle/macos/DICTER.app/Contents/MacOS/diction-mac --file audio.wav   # affiche le texte, code 0 si OK
```
Structure et protocole de maintenance : voir `docs/MAINTENANCE.md`. Variable `DICTION_URL` pour la vérification `--file`. Réglages enregistrés dans `~/Library/Application Support/com.bf.diction-mac/config.json`.

## Erreurs
Chaque erreur est une variante de `Erreur` (micro, passerelle hors ligne, passerelle, réponse, collage, réglage).
Elle s'affiche dans la pastille (ou en notification si la pastille est désactivée) et s'écrit dans `~/Library/Logs/DICTER.log` (horodatage et cause, jamais le texte dicté). Un fichier de réglages corrompu est mis de côté en `config.bad`.
