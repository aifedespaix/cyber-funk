#!/usr/bin/env bash
# Régénère assets/models/motos/moto-N-{high,low}.glb depuis assets/moto-src/moto-N/moto_raw.glb.
# Entrée (ignorée par git, voir assets/moto-src/README.md) : export de `normalize.py` (mètres, +Y haut, avant +Z).
# Prérequis : node (npx télécharge @gltf-transform/cli au premier lancement).
# Usage : bash tools/moto/build-glb.sh [1 2 3]   (sans argument : les trois motos)
set -euo pipefail
cd "$(dirname "$0")/../.."
G="npx -y @gltf-transform/cli@latest"
OUT=assets/models/motos
mkdir -p "$OUT"
# Ratio de triangles conservés (high, low) : les sources font 86 k, 375 k et 306 k triangles ; on vise ≈ 80 k / 35 k,
# le budget de la voiture, pour qu'une moto puisse un jour la remplacer.
declare -A HIGH=([1]=1.0 [2]=0.22 [3]=0.27)
declare -A LOW=([1]=0.45 [2]=0.10 [3]=0.12)
# Taille des textures (high, low) : les motos 1 et 2 ont 6 et 19 jeux de textures 4k non regroupés, d'où des tailles plus
# basses pour rester sous quelques Mo ; la moto 3 a des textures plus petites.
declare -A TEX_HIGH=([1]=1024 [2]=1024 [3]=2048)
declare -A TEX_LOW=([1]=512 [2]=512 [3]=1024)
for n in ${@:-1 2 3}; do
  RAW=assets/moto-src/moto-$n/moto_raw.glb
  T=$(mktemp -d)
  $G dedup "$RAW" "$T/d.glb"
  $G prune "$T/d.glb" "$T/p.glb"
  for q in high low; do
    if [ $q = high ]; then SIZE=${TEX_HIGH[$n]}; Q=85; R=${HIGH[$n]}; else SIZE=${TEX_LOW[$n]}; Q=78; R=${LOW[$n]}; fi
    $G resize "$T/p.glb" "$T/r.glb" --width $SIZE --height $SIZE
    if [ "$R" != "1.0" ]; then $G simplify "$T/r.glb" "$T/s.glb" --ratio "$R" --error 0.01; else cp "$T/r.glb" "$T/s.glb"; fi
    $G webp "$T/s.glb" "$T/w.glb" --quality $Q
    $G meshopt "$T/w.glb" "$OUT/moto-$n-$q.glb" --level high
  done
  rm -rf "$T"
done
ls -la "$OUT"/*.glb
