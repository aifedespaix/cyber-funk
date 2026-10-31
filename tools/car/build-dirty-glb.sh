#!/usr/bin/env bash
# Régénère assets/models/car/car-{high,low}.glb depuis l'export Blender de la « dirty car » (volant à gauche).
# Entrées (ignorées par git, voir assets/car-src/README.md) :
#   assets/car-src/dirty_raw.glb              export Blender de DirtyCar_Game.blend (glTF 2.0, +Y haut, matériaux)
#   assets/car-src/dirty_bake/paint_mask.png  masque de peinture (blanc = tôle repeinte), peint depuis les UV
# Prérequis : node, `npm i` une fois dans tools/car/.
# Même chaîne que build-glb.sh, sans le masque calculé par teinte (la dirty car est grise, pas violette : la teinte de
# référence du shader est portée par la texture de couleur `dirty_bake/body_color.png`, déjà teintée dans le masque).
set -euo pipefail
cd "$(dirname "$0")/../.."
SRC=assets/car-src
OUT=assets/models/car
G="npx -y @gltf-transform/cli@latest"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
node tools/car/inject-mask.mjs "$SRC/dirty_raw.glb" "$SRC/dirty_bake/paint_mask.png" "$T/masked.glb"
# high : 2k WebP + meshopt
$G resize "$T/masked.glb" "$T/h1.glb" --width 2048 --height 2048
$G webp "$T/h1.glb" "$T/h2.glb" --quality 90
$G meshopt "$T/h2.glb" "$OUT/car-high.glb" --level high
# low : 1k WebP, 60 % des triangles (la dirty car est déjà légère : ~18 000 triangles) + meshopt
$G resize "$T/masked.glb" "$T/l1.glb" --width 1024 --height 1024
$G simplify "$T/l1.glb" "$T/l2.glb" --ratio 0.6 --error 0.002
$G webp "$T/l2.glb" "$T/l3.glb" --quality 80
$G meshopt "$T/l3.glb" "$OUT/car-low.glb" --level high
ls -la "$OUT"/*.glb
