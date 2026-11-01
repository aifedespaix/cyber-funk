#!/usr/bin/env bash
# Régénère assets/models/characters/{manul,robot}.glb depuis assets/char-src/<nom>/char.blend.
# Prérequis : Blender 5.x, python (Pillow, numpy), node (`npm i` une fois dans tools/car/).
# Usage : bash tools/char/build-glb.sh [manul robot]
set -euo pipefail
cd "$(dirname "$0")/../.."
BLENDER="${BLENDER:-/c/Program Files/Blender Foundation/Blender 5.1/blender.exe}"
G="npx -y @gltf-transform/cli@latest"
OUT=assets/models/characters
mkdir -p "$OUT"
# Teinte de la carrosserie d'origine (degrés), saturation minimale, triangles conservés, taille des textures.
declare -A TEX_BASE=([manul]=textures/RobotMANUL_Base_color.png [robot]=textures/T2.png)
declare -A H0=([manul]=18 [robot]=35)
declare -A H1=([manul]=45 [robot]=55)
declare -A SMIN=([manul]=0.55 [robot]=0.45)
declare -A RATIO=([manul]=1.0 [robot]=0.5)
for c in ${@:-manul robot}; do
  D=assets/char-src/$c
  T=$(mktemp -d)
  "$BLENDER" -b -P tools/char/export.py -- "$D" > /dev/null
  python -I tools/char/make_mask.py "$D/${TEX_BASE[$c]}" ${H0[$c]} ${H1[$c]} ${SMIN[$c]} "$T/mask.png"
  node tools/car/inject-mask.mjs "$D/char_raw.glb" "$T/mask.png" "$T/m.glb" body_paint
  $G dedup "$T/m.glb" "$T/d.glb"
  $G resize "$T/d.glb" "$T/r.glb" --width 1024 --height 1024
  if [ "${RATIO[$c]}" != "1.0" ]; then $G simplify "$T/r.glb" "$T/s.glb" --ratio "${RATIO[$c]}" --error 0.01; else cp "$T/r.glb" "$T/s.glb"; fi
  $G webp "$T/s.glb" "$T/w.glb" --quality 85
  # Pas de meshopt : son passage de `prune` supprime le squelette (Skin) des personnages. Ils sont légers sans lui.
  cp "$T/w.glb" "$OUT/$c.glb"
  rm -rf "$T"
done
ls -la "$OUT"/*.glb
