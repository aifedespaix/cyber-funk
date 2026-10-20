# sillage-golden/road-render

Rendu de référence de la route de Sillage (three.js), collé à côté des captures de cyber-funk, pour le critère
« capture côte à côte Sillage / cyber-funk » de J5-01 (`docs/tasks/J5.md`).

Contrairement à `../postfx/` (qui réécrit la scène), la route est ici **la classe `Road` d'origine** : `road.ts` et ses
trois dépendances sont des copies conformes dans `sillage/engine/` (commit `fd344819121f292dd0646289a898c049b2e9d67f`
de la branche `main` de `aifedespaix/aifedespaix.com`, copiées le 2026-10-20) :

| Copie conforme | Blob git |
| --- | --- |
| `sillage/engine/road.ts` | `3f215fa7a98d` |
| `sillage/engine/world-origin.ts` | `24ec895deb27` |
| `sillage/engine/landmarks/field.ts` | `6fffa0e33804` |
| `sillage/engine/landmarks/glsl.ts` | `7b310cdb9803` |

La route est nourrie comme `sillage-scene.ts` le fait (`road.update(voiture, temps, pouls, basses, lueur, teinte)`,
`setDraw`, `setBrake`, `uWet`, `uRain`) avec les réglages que cyber-funk a réellement utilisés (`scenes.json`, écrit par
l'exemple), puis rendue par `RenderPass` + `OutputPass` (HDR demi-flottant, conversion sRGB). Ni bloom ni étalonnage :
ce critère ne compare que la route. La caméra, le pavé du point suivi et le fond noir sont ceux de
`crates/cf-world-neon/examples/road_demo` (à garder identiques).

## Régénérer

Prérequis : `bun`, `node` (Playwright se lance avec `node`), un Chromium de Playwright, et pour cyber-funk Xvfb et Mesa
(rendu logiciel).

```bash
# 1. cyber-funk : scènes scriptées, captures, réglages et verdicts sur les pixels
CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/route VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  xvfb-run -a cargo run -p cf-world-neon --example road_demo --features demo

# 2. Sillage : mêmes scènes, mêmes réglages, puis images côte à côte et écarts par zone
cd tools/sillage-golden/road-render
bun install && bun run build
bunx playwright install chromium-headless-shell
node render.mjs /tmp/route
```

Sortie dans `/tmp/route` : `sillage-<scène>.png`, `cote-a-cote-<scène>.png` (Sillage à gauche, cyber-funk à droite), et
sur la console l'écart moyen (0-255, sRGB) sur cinq zones et sur l'image entière, avec la part des pixels à plus de 8/255.

## Lire les écarts

- Les **moyennes par zone** coïncident à un ou deux niveaux près : mêmes couleurs, même luminosité.
- Les **pixels à plus de 8/255** (1 % de l'image) sont les liserés d'un pixel de large (crénelage différent entre les
  rastériseurs) et les gouttes de pluie : leur position vient d'un hash `sin(dot(…) × 43758)` sur de grands arguments,
  que chaque GPU évalue à sa façon. C'est vrai de Sillage d'une machine à l'autre.
- Au-delà de 14 400 m, Sillage change brutalement la teinte des liserés et des joints à chaque recentrage ; cyber-funk
  garde la teinte continue (voir l'en-tête de `assets/shaders/cf-world-neon/road.wesl`). Les scènes de départ (origine 0)
  sont donc identiques bit à bit sur ce point, pas celles du km 403.
