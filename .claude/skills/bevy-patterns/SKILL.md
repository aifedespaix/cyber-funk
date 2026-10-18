---
name: bevy-patterns
description: Use when writing Bevy code in cyber-funk (plugins, systems, resources, messages, windows, schedules) or when upgrading Bevy to a new version.
---

# Bevy dans cyber-funk

**Toujours vérifier l'API de la version en place** (`Cargo.toml` du workspace) via Context7 ou docs.rs : Bevy casse
son API à chaque version, et la mémoire des modèles a souvent une ou deux versions de retard (ex. événements devenus
`Message`, changements de `Camera`, de `RenderLayers`, des imports de shaders).

## Organisation

- Un crate Bevy = un ou plusieurs `Plugin`. Le binaire `cyber-funk` ne fait qu'assembler les plugins et lire la config.
- Ordre de l'image en `SystemSet` explicites, déclarés dans `cf-render` et réutilisés partout :

```
Update:
  CfSet::Input      lecture Actions (clavier, MIDI…), AudioFeatures du thread d'analyse
  CfSet::Show       chef du spectacle : publie ShowState
  CfSet::Simulate   monde : voiture, trafic, cascades, événements, streaming des tronçons
  CfSet::Direct     réalisateur caméra : pose de caméra
  CfSet::Present    palettes, effets, couches, UI opérateur
```

  `.chain()` entre ensembles, parallélisme libre à l'intérieur.
- La logique métier reste dans les crates sans Bevy ; le système Bevy appelle `logic.update(&inputs)` et applique le
  résultat aux composants. Un système fait une chose.

## Ressources et état

- `ShowState`, `AudioFeatures`, `CinemaWorld`, `Palette` : `Resource`, écrites par **un seul** système.
- Ordres ponctuels (drop forcé, plan suivant…) : messages Bevy, lus par le système concerné.
- Pas de `World` exclusif dans les systèmes par image sauf nécessité (ça casse le parallélisme).
- Change detection (`Changed<T>`, `Res::is_changed`) pour ne recalculer que ce qui bouge.

## Entités

- Monde streamé : entités recyclées via pool (composant `Pooled`, `Visibility::Hidden` quand libre), jamais de
  `spawn`/`despawn` en régime établi.
- Hiérarchies : garder peu profondes (chaque niveau coûte en propagation de transform).
- Marqueurs de couche : `RenderLayers` (voir `render-performance`).

## Fenêtres

- Fenêtre principale = **projecteur** (plein écran sans bordure, choix de l'écran par config/CLI, curseur caché, pas
  d'UI). Fenêtre secondaire = **opérateur** (egui). Fermer l'opérateur ne coupe pas le projecteur ; une confirmation
  est demandée pour quitter.
- `PresentMode::AutoVsync` par défaut, `Immediate` en option de diagnostic.

## Feature `dev`

Feature cargo `dev` dans le binaire : `bevy/dynamic_linking`, `bevy/file_watcher` (rechargement à chaud des assets,
shaders et looks RON), surcouche de diagnostic. Jamais activée en release.

## Monter de version Bevy

1. PR dédiée `chore/bevy-0.X`, rien d'autre dedans.
2. Lire le guide de migration officiel en entier ; noter les points qui nous touchent dans la PR.
3. Monter `bevy` et **toutes** les `bevy_*` (egui, etc.) ensemble ; attendre que chaque crate tierce publie sa version
   compatible.
4. `cargo build`, `clippy`, tests, puis lancer l'app et vérifier à l'œil : couches, post-process, culling (compteurs de
   la surcouche), fenêtre opérateur.
5. Si une crate tierce bloque : ne pas forker, attendre ou demander au propriétaire du dépôt.
