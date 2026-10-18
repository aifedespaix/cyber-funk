---
name: render-performance
description: Use when touching rendering in cyber-funk - culling (frustum/occlusion/simulation), world streaming and pools, floating origin, render layers (background/world/foreground/overlay), post-process, WGSL shaders, GPU budget, adaptive quality.
---

# Rendu et performances

## Budget

- Cible : **1080p 60 i/s stables** sur GPU milieu de gamme (classe RTX 3060 / Apple M1 Pro). 4K 30 i/s en option.
- Budget image 16,6 ms. Mesurer **au GPU** (requêtes de timestamp, diagnostics de rendu Bevy), pas seulement au CPU.
- Toute tâche de rendu indique dans sa PR : i/s, ms GPU, draw calls, entités visibles / cullées, avant/après, sur la
  scène de référence de la surcouche de diagnostic.

## Culling : quatre niveaux

1. **Frustum** (natif) : chaque mesh a un `Aabb` juste. Les meshes qui suivent la caméra (brume, pluie, traînée,
   particules) reçoivent un `Aabb` recalculé ou assez grand — **jamais** `NoFrustumCulling` sans justification écrite.
2. **Occlusion** : occlusion culling GPU de Bevy activé sur la caméra « monde » (depth prepass requis). Vérifier son
   statut (expérimental ou non) dans la version en place ; il reste désactivable par réglage.
3. **Simulation** : les systèmes coûteux filtrent sur `ViewVisibility` ou une distance ; hors champ = cadence réduite
   ou gel. Les décisions de mise en scène (événements, cascades) restent calculées, seule l'animation est gelée.
4. **Distance / LOD** : `VisibilityRange` pour fondre les LOD ; au-delà de la brume, rien n'est dessiné.

Le culling économise du **calcul**. La **mémoire** se tient par le streaming + pools : nombre d'entités et de buffers
GPU constants en régime établi, zéro allocation par image.

## Streaming du monde

- La route file vers −z (repère de Sillage). Le monde est découpé en tronçons de longueur fixe ; une fenêtre de
  tronçons vit autour de la caméra (≈ 90 m derrière, ≈ 490 m devant : bornes de Sillage, `cinema/director.ts`).
- Un tronçon qui sort derrière est recyclé devant (pool), contenu re-tiré par graine déterministe (`hash(index)`).
- **Origine flottante** : Bevy calcule en `f32`. À 400 km du départ (fin de set), la précision tombe au centimètre et
  les shaders tremblent. On recentre périodiquement tout le monde (caméra comprise) autour de l'origine, d'un multiple
  de `ORIGIN_STEP = 28800` m (repris de `world-origin.ts`) pour que les motifs réguliers restent continus. Les
  coordonnées « absolues » (en `f64`) ne vivent que dans la logique.

## Couches de rendu

| Ordre | Couche | `RenderLayers` | Profondeur |
| --- | --- | --- | --- |
| 0 | Arrière-plan : ciel, étoiles, planètes | 0 | à part, parallaxe (`skyDrift` de Sillage) |
| 1 | Monde | 1 | normale |
| 2 | Avant-plan 3D : objets en repère caméra (logo DJ, particules devant l'objectif) | 2 | effacée avant cette passe |
| 3 | Surcouche 2D : titres, nom du DJ | 3 | aucune |

Toutes les caméras partagent la pose du réalisateur ; seules l'ordre, la couche, le `ClearColorConfig` et la
profondeur changent. Chaque couche a une opacité pilotée par le look. Post-process sur l'image composée.

## Post-process

HDR → bloom (à résolution réduite, diviseur réglé par la qualité) → grade (portage de `postfx-grade.ts`) → effets du
show (séparation RVB, flash **via le limiteur**) → tonemapping → sortie. Chaque passe custom = un nœud du render graph
dans `cf-render`, avec ses réglages en `Resource` extraite vers le monde de rendu.

## Shaders

- WGSL dans `assets/shaders/<crate>/<nom>.wgsl`, rechargés à chaud avec la feature `dev`.
- Pas de branche dynamique coûteuse par pixel quand une variante de pipeline (`#ifdef` via `shader_defs`) suffit.
- **Préchauffage** : tous les pipelines utilisés par la liste de cues sont compilés à l'ouverture du show (scène de
  préchauffe hors écran), jamais au premier affichage.

## Instancing et textures

- Même `Mesh` + même matériau = batching automatique. Éviter de multiplier les matériaux : préférer un matériau
  paramétré par attributs/instance ou par atlas.
- Atlas de textures pour façades et enseignes ; textures compressées (KTX2) quand c'est stable.

## Qualité adaptative

Portage de `quality.ts` : sous ~30 i/s on dégrade d'un cran (échelle de rendu 1 → 0,9 → 0,8 → 0,7, puis bloom, puis
particules), remontée prudente avec patience qui double à chaque rechute. Jamais visible comme un saut brutal :
transitions sur une mesure.
