# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions : [SemVer](https://semver.org). Tant que la version commence par 0, tout peut changer.

## [Non publié]

## [0.6.0]
### Added
- « Baisser les autres sons » (`duck_others`, `duck_percent`) : le volume système descend à N % de sa valeur au début de l'enregistrement et revient à la fin, sur annulation, erreur, arrêt automatique, panique et fermeture de l'app. Un fichier `duck-pending` restaure aussi le volume au lancement après un plantage. Volume modifié par l'utilisateur pendant la dictée ou sortie muette : rien n'est touché. Code d'erreur `E-DUCK`.
- « Afficher le texte reconnu » (`show_result`, `result_seconds` de 1 à 10) : le texte apparaît dans la pastille avant qu'elle se masque. Il n'est jamais écrit dans le journal.
- Version du fichier de réglages : 2 (anciens fichiers lus sans changement, nouveaux réglages à faux).

## [0.5.0]
### Added
- Mode d'insertion « Terminal » (⌃⇧V) et réglage « Copier aussi dans le presse-papiers » (`copy_to_clipboard`).
- Règles personnalisées : préfixe `[exact]` (mot entier, casse respectée) ou `[smart]` (casse et ponctuation voisine ignorées), sélecteur par règle dans les réglages. Sans préfixe, comportement inchangé. Règle invalide : ignorée et journalisée.
- Taille de la pastille (`overlay_size` : small, medium, large ; medium = taille d'origine).
- Raccourcis : boutons « Effacer » (sauf l'enregistrement) et « Rétablir le défaut » (valeur tirée de `Config::default`, commande `default_config`).
- Test de migration renforcé : `sounds` recopié sur `sound_start` et `sound_stop`.

### Changed
- Un échec du collage ou de la frappe laisse le texte dans le presse-papiers (code E-PASTE).
- Raccourcis vides permis pour « Coller la dernière », « Annuler » et « Commande » ; ils ne comptent pas comme doublons.

## [0.4.1]
### Security
- Adresses `url` et `command_url` : `http://` ou `https://` complet exigé (`httpx://` refusé).
- Import de réglages : un hôte distant (hors machine locale, 192.168.x, 10.x, Tailscale) différent de l'hôte enregistré est refusé. Message d'erreur d'import générique.
- Politique de sécurité du contenu (CSP) restrictive dans la fenêtre de réglages.
### Added
- Arrêt automatique d'un enregistrement après 300 s, avec notification.
- Option « Texte sur une seule ligne » (retours à la ligne remplacés par des espaces au collage).
- Fichier `LICENSE` (MIT).
### Changed
- Profil release optimisé (LTO, strip, opt-level s).

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
