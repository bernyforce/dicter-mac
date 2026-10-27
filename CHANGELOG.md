# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions : [SemVer](https://semver.org). Tant que la version commence par 0, tout peut changer.

## [Non publié]

## [0.4.0]
### Added
- Mode Commande (désactivé par défaut) : une instruction dictée transforme le texte sélectionné via un modèle local compatible OpenAI. Raccourci `Ctrl+Maj+X`, code d'erreur `E-LLM`, texte laissé intact en cas d'échec.


## [0.3.0]
### Added
- Espace de travail Cargo en quatre modules : `diction-core`, `diction-audio`, `diction-platform`, `diction-mac`.
- Vérification des mises à jour par les releases GitHub (réglages « Mises à jour »).
- Pastille avec signal oscillant, animée à partir du niveau réel du micro.
- Dictionnaire (indice de reconnaissance envoyé au serveur) et règles de formatage (texte court, espace avant ? et !, espace final, hésitations, règles personnalisées).
- Raccourcis « Coller la dernière transcription » et « Annuler l'enregistrement » (actif seulement pendant l'enregistrement).
- Choix du micro, insertion automatique, mode d'insertion (standard, direct, copie), effets sonores, démarrage à l'ouverture de session, Dock, historique en option, conservation des 5 derniers audios.
- Import et export des réglages (le nom du micro n'est jamais exporté).


## [0.2.1]
### Added
- Codes d'erreur stables, crochet de panique, export du diagnostic.
### Fixed
- La fenêtre de réglages s'ouvre au lancement et à la réouverture de l'app.
- Les tests n'écrivent plus dans le vrai journal.

## [0.2.0]
### Added
- Refonte Tauri 2 : fenêtre de réglages, pastille, raccourci configurable, options de collage.

## [0.1.0]
### Added
- Première version : raccourci ⌥ Espace, micro, envoi à la passerelle, collage.
