# cyber-funk

Visuels 3D temps réel pour soirées DJ. Une ville néon filmée par un réalisateur automatique qui coupe sur les temps,
prépare les drops et monte en intensité au fil de la nuit, pendant que les DJs se succèdent aux platines.

Application native en Rust, construite sur [Bevy](https://bevyengine.org). C'est le portage, repensé pour le live, de
**Sillage**, l'expérience web de [aifedespaix.com/sillage](https://aifedespaix.com/sillage).

> **État : démarrage.** L'architecture est validée et découpée en tâches ; le code arrive jalon par jalon.
> Voir [`docs/TASKS.md`](docs/TASKS.md) pour l'avancement.

---

## Ce que ça fait (cible)

- **Réagit à la musique en direct** : micro ou entrée ligne, détection des kicks, du tempo et des sections
  (calme, groove, sommet), drops anticipés.
- **Se cale sur le logiciel de mix** : MIDI Clock (Traktor, Rekordbox, Serato…), Ableton Link en option.
- **Se pilote au clavier, au contrôleur MIDI ou depuis la table du DJ** : GO / BACK, macros sur potards, pads
  d'effets, MIDI learn.
- **Plusieurs DJs par soirée** : un créneau par DJ avec son nom, son logo, ses couleurs, sa liste de cues et son
  profil de contrôleur ; passation mise en scène.
- **Préparer chez soi, jouer sur place** : la soirée tient dans un fichier `.cfshow` (RON) ; reprise automatique
  après un plantage.
- **Deux fenêtres** : la sortie projecteur, sans aucune interface, et la fenêtre opérateur.
- **Pensé pour le public** : limiteur photosensible (WCAG 2.3.1) toujours actif, look de secours, blackout.
- **Optimisé GPU** : frustum et occlusion culling, monde streamé par tronçons recyclés, qualité adaptative, cible
  1080p à 60 i/s sur une carte graphique milieu de gamme.

## Documentation

| Document | Contenu |
| --- | --- |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Choix du moteur, pipeline audio/MIDI, culling, couches de rendu, modèle Événement → Créneau → Cue → Look, plan par jalons |
| [`docs/TASKS.md`](docs/TASKS.md) | Les 67 tâches, le modèle d'agent conseillé pour chacune, dépendances et vagues de parallélisation |
| [`docs/tasks/`](docs/tasks/) | Fiche détaillée de chaque tâche (périmètre, sources Sillage, critères d'acceptation) |
| [`CLAUDE.md`](CLAUDE.md) | Règles du dépôt pour les agents (et les humains) |
| [`.claude/skills/`](.claude/skills/) | Savoir-faire du projet : conventions Rust, Bevy, audio temps réel, MIDI, modèle du show, portage depuis Sillage, performance, sécurité en live |

## Prérequis

- **Rust stable** récent (installé via [rustup](https://rustup.rs) ; la version est gérée par `rust-toolchain.toml`).
- **Linux** : `sudo apt install libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev pkg-config`
- **macOS** : Xcode Command Line Tools.
- **Windows** : Visual Studio Build Tools (C++). Pour relier un logiciel de mix sur la même machine :
  [loopMIDI](https://www.tobias-erichsen.de/software/loopmidi.html).
- Outils conseillés : `cargo install cargo-nextest cargo-deny cargo-outdated`.

## Commandes

Disponibles à partir de la tâche J0-01 :

```bash
cargo run -p cyber-funk                          # lancer l'app
cargo run -p cyber-funk -- --windowed            # projecteur en fenêtre (dev)
cargo run -p cyber-funk -- --show assets/shows/demo.ron
cargo run -p cyber-funk --features dev           # rechargement à chaud, compilation plus rapide

cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo deny check
```

## Organisation

```
crates/
  cf-audio       capture et lecture audio            ┐
  cf-analysis    kicks, tempo, sections               │ logique pure,
  cf-control     clavier, MIDI, clock, Link, OSC      │ sans Bevy
  cf-show        modèle du show et chef du spectacle  ┘
  cf-render      couches, post-process, streaming, culling
  cf-cinema      réalisateur caméra
  cf-world-neon  la ville de Sillage
  cf-operator    fenêtre opérateur
  cyber-funk     binaire
assets/          modèles, shaders, musiques de démo (libres), looks, shows
docs/            architecture, tâches
```

## Contribuer

Une tâche de `docs/TASKS.md` = une branche `task/<id>-<slug>` = une PR vers `main`. La CI (format, clippy, tests,
cargo-deny, compilation Linux/Windows/macOS) doit être verte. Les règles sont dans [`CLAUDE.md`](CLAUDE.md).

## Musiques

Seules des pistes à licence libre sont versionnées (`assets/music/`, licences dans `assets/music/LICENCES.md`).
Pour tester avec vos propres morceaux, déposez-les dans `assets/music/local/`, ignoré par git.

## Licence

À définir.
