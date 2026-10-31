#!/usr/bin/env bash
# Régénère assets/models/car/car-{high,low}.glb depuis l'export brut de Blender.
# Entrées (ignorées par git, voir assets/car-src/README.md) :
#   assets/car-src/car_raw.glb        export Blender (glTF 2.0, +Y haut, matériaux)
#   assets/car-src/2k/COLOR2Kpng.png  diffuse, sert à calculer le masque de peinture
# Prérequis : python (numpy, pillow), node, `npm i` une fois dans tools/car/.
# Ne pas remplacer par des commandes gltf-transform seules : le masque serait perdu.
set -euo pipefail
cd "$(dirname "$0")/../.."
SRC=assets/car-src
OUT=assets/models/car
G="npx -y @gltf-transform/cli@latest"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
python -I tools/car/make_paint_mask.py "$SRC/2k/COLOR2Kpng.png" "$T/mask.png"
node tools/car/inject-mask.mjs "$SRC/car_raw.glb" "$T/mask.png" "$T/masked.glb"
# high : 2k WebP + meshopt
$G resize "$T/masked.glb" "$T/h1.glb" --width 2048 --height 2048
$G webp "$T/h1.glb" "$T/h2.glb" --quality 90
$G meshopt "$T/h2.glb" "$OUT/car-high.glb" --level high
# low : 1k WebP, 40 % des triangles + meshopt
$G resize "$T/masked.glb" "$T/l1.glb" --width 1024 --height 1024
$G simplify "$T/l1.glb" "$T/l2.glb" --ratio 0.4 --error 0.002
$G webp "$T/l2.glb" "$T/l3.glb" --quality 80
$G meshopt "$T/l3.glb" "$OUT/car-low.glb" --level high
ls -la "$OUT"/*.glb
