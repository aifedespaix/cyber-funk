#!/usr/bin/env bash
# Pilote figé dans la géométrie : régénère les GLB des motos avec leur pilote et le conducteur de la voiture.
#   assets/models/motos/moto-N-{high,low}.glb      moto + manul posé (pose_moto N), via tools/moto/build-glb.sh
#   assets/models/characters/driver-car.glb        manul posé au volant (pose_car), à ajouter à la voiture
# Pourquoi figé : le lecteur de GLB du jeu ne lit que des maillages statiques (voir car/glb.rs).
# Prérequis : `bash tools/char/build-glb.sh` (poses dans char_posed.blend), Blender 5.x, python (Pillow, numpy), node.
# Usage : bash tools/char/build-riders.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
BLENDER="${BLENDER:-/c/Program Files/Blender Foundation/Blender 5.1/blender.exe}"
G="npx -y @gltf-transform/cli@latest"
CHAR=assets/char-src/manul
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
python -I tools/char/make_mask.py "$CHAR/textures/RobotMANUL_Base_color.png" 18 45 0.55 "$T/mask.png"
for n in 1 2 3; do
  "$BLENDER" -b -P tools/char/bake_rider.py -- "$CHAR" pose_moto$n "$T/r$n.glb" assets/moto-src/moto-$n/moto.blend | grep RAPPORT
  node tools/car/inject-mask.mjs "$T/r$n.glb" "$T/mask.png" assets/moto-src/moto-$n/moto_rider.glb body_paint
done
bash tools/moto/build-glb.sh
"$BLENDER" -b -P tools/char/bake_rider.py -- "$CHAR" pose_car "$T/driver.glb" | grep RAPPORT
node tools/car/inject-mask.mjs "$T/driver.glb" "$T/mask.png" "$T/driver_m.glb" body_paint
$G resize "$T/driver_m.glb" "$T/d1.glb" --width 1024 --height 1024
$G webp "$T/d1.glb" "$T/d2.glb" --quality 85
$G meshopt "$T/d2.glb" assets/models/characters/driver-car.glb --level high
ls -la assets/models/characters/driver-car.glb assets/models/motos/*.glb
