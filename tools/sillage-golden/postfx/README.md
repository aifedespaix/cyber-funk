# sillage-golden/postfx

Rendu de référence du post-process de Sillage (three.js), collé à côté des captures de cyber-funk, pour le critère
« capture côte à côte Sillage / cyber-funk » de J4-02 (`docs/tasks/J4.md`).

La chaîne est celle du constructeur de `PostFX` de Sillage (`engine/postfx.ts`) : cible HDR demi-flottante,
`RenderPass`, `UnrealBloomPass(résolution / 2, 0.3, 0.22, 0.85)`, `ShaderPass(GRADE)`, `OutputPass`,
`NeutralToneMapping` à l'exposition 1. Le shader est **la copie conforme** de `engine/postfx-grade.ts`
(`../sillage/engine/postfx-grade.ts`, blob git `c4534db67ff2`, copié de la branche `main` de
`aifedespaix/aifedespaix.com` le 2026-10-20). La scène (`scene.ts`) est celle de
`crates/cf-render/examples/postfx_demo` : les deux sont à garder identiques.

Les uniformes de chaque étape ne sont pas recalculés ici : ce sont ceux que cyber-funk a réellement envoyés à son
shader (`phases.json`, écrit par l'exemple). On compare deux rendus d'un même état, la logique des effets (limiteur,
étalonnage par chaleur…) étant testée à part en Rust.

## Régénérer

Prérequis : `bun`, `node` (Playwright se lance avec `node`), un Chromium de Playwright, et pour cyber-funk Xvfb et Mesa
(rendu logiciel).

```bash
# 1. cyber-funk : étapes scriptées, captures et uniformes
CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/postfx VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  xvfb-run -a cargo run -p cf-render --example postfx_demo --features demo

# 2. Sillage : même scène, mêmes uniformes, puis images côte à côte et écarts par zone
cd tools/sillage-golden/postfx
bun install && bun run build
bunx playwright install chromium-headless-shell
node render.mjs /tmp/postfx
```

Sortie dans `/tmp/postfx` : `sillage-<étape>.png`, `cote-a-cote-<étape>.png`, et sur la console l'écart moyen
(0-255, sRGB) entre les deux rendus sur quatre zones (ciel, charte, halo d'un néon, reflet de la route).
