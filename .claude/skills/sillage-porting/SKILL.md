---
name: sillage-porting
description: Use when translating a Sillage module (TypeScript/Three.js/GLSL from aifedespaix/aifedespaix.com apps/main/src/components/sillage) into Rust/Bevy/WGSL for cyber-funk, or when producing golden reference data from Sillage.
---

# Porter un module de Sillage

Source : dépôt `aifedespaix/aifedespaix.com`, dossier `apps/main/src/components/sillage/`
(moteur dans `engine/`, assets dans `apps/main/public/{models,music,sillage}`, couleurs des thèmes dans
`packages/tokens/src/scene.css`).

## Méthode

1. **Lire tout le module et ses imports directs** avant d'écrire une ligne. Repérer : l'état, ce qu'il lit
   (`ShowState`, `BeatState`, `CinemaWorld`…), ce qu'il écrit, les constantes réglées à la main.
2. **Garder les noms** (types, champs, constantes) en version Rust : `ShowGates` → `ShowGates`, `cutGrid` →
   `cut_grid`, `MAX_LATE` → `MAX_LATE`. Garder les commentaires de Sillage (traduits tels quels, ils expliquent des
   réglages).
3. **Reprendre les constantes à l'identique.** Toute modification se justifie dans la PR (section « Écarts »).
4. **Séparer logique et rendu** : la logique (décisions, timings, probabilités) va dans une struct pure testable ; la
   partie Three.js devient un plugin/système Bevy mince.
5. **Supprimer ce qui relève du mode Jeu** : volant, score, bonus, boîte de vitesses, commandes de vol, sons de voiture.
   Si le module mélange les deux, on ne porte que le chemin « cinéma / spectacle » (`cinemaOn`, `showState` actif).
6. **Tester contre Sillage** quand c'est déterministe : produire des données de référence avec un script dans
   `tools/sillage-golden/` (Bun + les fichiers TS de Sillage) et les comparer en Rust avec une tolérance explicite.

## Correspondances

| Sillage (TS / Three.js) | cyber-funk (Rust / Bevy) |
| --- | --- |
| `class X { update(dt) }` appelée dans `sillage-scene.ts` | `struct X` (`Resource` ou `Component`) + système dans un `SystemSet` ordonné |
| Objet `readonly` partagé (`ShowState`, `BeatState`) | `Resource` lue par `Res<T>` ; un seul système l'écrit (`ResMut<T>`) |
| `take(action)` à consommation unique | `Messages`/événements Bevy, ou drapeau consommé par le seul système concerné |
| `Vector3`, `Quaternion`, `Color` | `glam::Vec3`, `Quat`, `bevy::color::LinearRgba` / `Srgba` |
| `Math.random()` | RNG injecté avec graine (`rand_pcg::Pcg32`), jamais global |
| `InstancedMesh` | Entités partageant `Mesh3d` + `MeshMaterial3d` (batching auto) ; ou matériau instancié custom si > ~10 k |
| `ShaderMaterial` (GLSL) | `Material` / `ExtendedMaterial<StandardMaterial, _>` + fichier `.wgsl` dans `assets/shaders/` |
| `onBeforeCompile` sur un matériau standard | `ExtendedMaterial` |
| `CanvasTexture` (dessin 2D : enseignes, glyphes) | rendu CPU avec `tiny-skia` + texte `cosmic-text`, envoyé dans une `Image` ; généré au chargement, jamais par image |
| `EffectComposer` / passes | nœud(s) de post-process dans le render graph (`cf-render`) |
| `frustumCulled = false` | `Aabb` correct (voir skill `render-performance`), pas `NoFrustumCulling` |
| `localStorage` | fichier RON dans le dossier de données de l'app (`dirs::data_dir()`), écriture atomique |
| `world-origin.ts` (origine des motifs) | origine flottante globale de `cf-render` (recentrage du monde), voir `render-performance` |

## GLSL → WGSL, pièges courants

- `mod(x, y)` GLSL ≠ `%` WGSL pour les négatifs : écrire `x - y * floor(x / y)`.
- `fract`, `mix`, `smoothstep`, `step` existent ; `atan(y, x)` → `atan2(y, x)`.
- Pas de surcharge implicite `float`/`int` : caster explicitement.
- Uniforms → `@group(2) @binding(n)` côté matériau ; le temps vient de `globals.time` (import `bevy_pbr::mesh_view_bindings::globals`)
  ou d'un uniform du matériau si le show le pilote.
- Vérifier les imports de shader de la version de Bevy en place (ils changent entre versions) via Context7.

## Ne pas porter

`controls.ts`, `cockpit-controls.ts`, `gearbox.ts`, `shift-gate.ts`, `score.ts`, `pickups.ts`, `arches.ts`,
`flight/input.ts`, `race-line.ts`, `sfx.ts`, `car-preview.ts`, `debug-log.ts` (→ `tracing`), composants Svelte de jeu,
SEO, PWA.
