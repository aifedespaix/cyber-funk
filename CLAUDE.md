# CLAUDE.md - cyber-funk

> Source de vérité pour tout agent qui travaille sur ce dépôt. À lire en entier avant d'agir.
> Les règles marquées 🔴 ne souffrent aucune exception. Le détail vit dans les skills (§6).

---

## 0. Le projet en une phrase

Application native de VJing 3D temps réel (Rust + Bevy), portée de **Sillage**
(`aifedespaix/aifedespaix.com` → `apps/main/src/components/sillage/`), qui réagit en direct aux sets de plusieurs DJs
via le micro, le MIDI et les logiciels de mix. Un seul mode d'affichage : le **mode Cinéma**.

Documents de référence, à lire avant toute tâche :

- `docs/ARCHITECTURE.md` : choix techniques, modèle de données, découpage en crates (validé).
- `docs/TASKS.md` : liste des tâches, modèle d'agent recommandé, dépendances. Le détail de chaque tâche est dans
  `docs/tasks/J*.md`.

---

## 1. Règles non négociables 🔴

1. **Pas de `unsafe`** sans commentaire `// SAFETY:` qui justifie chaque invariant, et jamais dans la logique métier.
2. **Pas de `unwrap()` / `expect()` / `panic!`** hors tests et hors `main` au démarrage. Les erreurs remontent
   (`thiserror` dans les bibliothèques, `anyhow` dans le binaire).
3. **Thread audio temps réel : zéro allocation, zéro verrou, zéro log, zéro I/O** dans les callbacks `cpal`.
   Voir skill `realtime-audio`.
4. **Pas d'allocation par image en régime établi** dans les systèmes Bevy chauds (réutiliser les buffers, pools
   d'entités). Voir skill `render-performance`.
5. **Les crates `cf-audio`, `cf-analysis`, `cf-control`, `cf-show` ne dépendent pas de Bevy.** Elles restent
   testables seules et insensibles aux montées de Bevy.
6. **Pas de `NoFrustumCulling`** sans commentaire qui explique pourquoi et quel `Aabb` aurait été impossible.
7. **Tout flash, stroboscope ou forte variation de luminance passe par le limiteur photosensible** (`cf-show`,
   WCAG 2.3.1). Aucune exception, même « juste pour tester ».
8. **Aucun chargement d'asset ni compilation de shader pendant un show** : tout est préchargé à l'ouverture.
9. **Musiques** : seules des pistes à licence libre (CC0, CC-BY…) sont versionnées dans `assets/music/`, avec leur
   licence dans `assets/music/LICENCES.md`. Les autres vont dans `assets/music/local/` (ignoré par git).
10. **Avant chaque commit** : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
    `cargo nextest run --workspace` (ou `cargo test --workspace`). Tout doit être vert. On ne passe pas à la tâche
    suivante avec un avertissement ouvert.
11. **Fichiers de plus de 400 lignes** : à découper en modules (hors données générées et tests).
12. **Pas de secret** (clé, token) dans le code ou les fichiers de show.

---

## 2. Structure du dépôt

```
cyber-funk/
├── Cargo.toml                  workspace (lints et versions communes)
├── crates/
│   ├── cf-audio/               capture, lecture des démos, ring buffers            (sans Bevy)
│   ├── cf-analysis/            FFT, kicks, BPM, sections, AudioFeatures            (sans Bevy)
│   ├── cf-control/             Actions, clavier abstrait, MIDI, clock, Link, OSC   (sans Bevy)
│   ├── cf-show/                Événement/Créneau/Cue/Look, chef du spectacle,      (sans Bevy)
│   │                           portes, drop, fraîcheur, limiteur, sauvegarde
│   ├── cf-render/              plugin Bevy : couches, post-process, streaming, culling, qualité, palettes
│   ├── cf-cinema/              plugin Bevy : réalisateur, plans, transitions
│   ├── cf-world-neon/          plugin Bevy : le monde de Sillage
│   ├── cf-operator/            plugin Bevy + egui : fenêtre opérateur
│   └── cyber-funk/             binaire : assemble les plugins
├── assets/                     models/ shaders/ textures/ music/ looks/ shows/
├── tools/                      scripts annexes (ex. export des données de référence depuis Sillage)
├── docs/                       ARCHITECTURE.md, TASKS.md, tasks/
└── .claude/                    skills, hooks, réglages des agents
```

Sens des dépendances (jamais l'inverse) :

```
cf-audio ─▶ cf-analysis ─┐
cf-control ──────────────┼─▶ cf-show ─▶ cf-render ─▶ cf-cinema ─▶ cf-world-neon ─▶ cyber-funk
                         └──────────────────────────▶ cf-operator ───────────────────▶
```

Le type `Action` est défini dans `cf-control` ; `cf-show` le consomme.

---

## 3. Stack

🔴 **Politique de version : dernière version stable de chaque dépendance**, majeure incluse. Avant une montée majeure :
lire le changelog / guide de migration, puis `cargo build`, `cargo clippy`, `cargo test`. Si la montée casse quelque
chose de non trivial, demander avant de l'appliquer. **Bevy et toutes les crates `bevy_*` montent ensemble, dans une PR
dédiée** (voir skill `bevy-patterns`).

| Rôle | Crate | Version au 2026-10-18 (vérifier avec `cargo outdated` / `cargo search`) |
| --- | --- | --- |
| Moteur | `bevy` | 0.20 |
| Interface opérateur | `bevy_egui` | 0.43 (compatible avec la version de Bevy en place) |
| Audio E/S | `cpal` | 0.18 |
| Décodage | `symphonia` | 0.6 |
| FFT | `realfft` | 3.5 |
| Transport temps réel | `rtrb` 0.4, `triple_buffer` 9 | |
| MIDI | `midir` | 0.11 |
| Ableton Link (option) | `rusty_link` | 0.4 |
| OSC (option) | `rosc` | 0.11 |
| Données | `serde` 1, `ron` 0.12 | |
| Erreurs | `thiserror` 2 (libs), `anyhow` 1 (binaire) | |
| Journaux | `tracing` | |
| Tests | `cargo-nextest`, `insta` (instantanés), `proptest` (propriétés), `criterion` (bench) | |

Rust : dernier stable (`rust-toolchain.toml`), édition 2024.

---

## 4. Commandes

```bash
cargo run -p cyber-funk                     # lancer l'app (fenêtres projecteur + opérateur)
cargo run -p cyber-funk -- --show assets/shows/demo.ron
cargo run -p cyber-funk --features dev      # rechargement à chaud, liaison dynamique de Bevy

cargo fmt --all                             # format
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace               # tests (ou cargo test --workspace)
cargo test --workspace --doc                # doctests
cargo deny check                            # licences, failles, doublons
cargo bench -p cf-analysis                  # benchmarks
```

Dépendances système Linux : `libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev pkg-config`
(installées automatiquement en session cloud par `.claude/hooks/session-start.sh`).

---

## 5. Façon de travailler

- **Une tâche = une branche = une PR.** Branche `task/<id>-<slug>` (ex. `task/J1-04-beat-detector`), sauf si la
  session impose un autre nom. Titre de PR : `[J1-04] Détecteur de kicks`.
- Lire la fiche de la tâche dans `docs/tasks/J*.md`, charger les skills qu'elle cite, lire les fichiers Sillage
  qu'elle cite **avant** d'écrire du code.
- Respecter le périmètre de la tâche. Un problème hors périmètre se note dans la PR (section « Écarts »), il ne se
  corrige pas en douce.
- Cocher les critères d'acceptation dans la PR, un par un, avec la preuve (test, sortie de commande, capture).
- Mettre à jour `docs/TASKS.md` (statut de la tâche) dans la même PR.
- **Commentaires et docs en français**, identifiants en anglais (convention Rust). Commenter le *pourquoi*, pas le
  *quoi*, au même niveau de détail que Sillage.
- Les noms et contrats de Sillage (`ShowState`, `ShotSpec`, `Gates`, `Freshness`…) sont conservés pour garder la
  correspondance lisible.

---

## 6. Skills du projet

Charger le skill dès que la tâche touche son domaine :

| Skill | Quand |
| --- | --- |
| `rust-conventions` | Toujours : style, erreurs, lints, tests, organisation des crates |
| `workflow-conventions` | Choisir un modèle, lancer une tâche, ouvrir une PR |
| `sillage-porting` | Traduire un module TypeScript/GLSL de Sillage en Rust/WGSL |
| `bevy-patterns` | Plugins, systèmes, ressources, messages, montée de version de Bevy |
| `render-performance` | Culling, streaming, instancing, budget GPU, shaders WGSL, post-process |
| `realtime-audio` | `cf-audio`, `cf-analysis`, threads audio, latence |
| `midi-control` | `cf-control` : Actions, MIDI, clock, Link, OSC, learn |
| `show-model` | `cf-show` : Événement/Créneau/Cue/Look, portes, drop, fichiers RON |
| `live-safety` | Limiteur de flashs, replis, sauvegarde, préchargement, tests d'endurance |
