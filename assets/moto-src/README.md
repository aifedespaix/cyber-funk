# Sources des motos (non versionnées, sauf ce fichier)

Ce dossier est ignoré par git (≈ 450 Mo : un FBX de 178 Mo et des textures 4k). Il contient, pour chaque moto :

| Fichier | Rôle |
| --- | --- |
| `moto-N/source/*.fbx` | modèle d'origine, tel que téléchargé |
| `moto-N/textures/` | textures d'origine (le FBX ne les référence pas) |
| `moto-N/moto.blend` | **scène de travail** : matériaux reliés aux textures, échelle et orientation normalisées, nœud racine `moto` |
| `moto-N/moto_raw.glb` | export brut du `.blend` (glTF 2.0, +Y haut), entrée de la chaîne d'optimisation |

| Moto | Origine | Triangles | Avant du modèle d'origine |
| --- | --- | --- | --- |
| `moto-1` | `Moto_full_model.fbx` (roues sans moyeu, avant à gauche) | 86 k | −X |
| `moto-2` | `motur_lower.fbx` (armée, canons sur le garde-boue) | 375 k | −Y |
| `moto-3` | `Projeto_MX.fbx` (carénage noir, roues sans moyeu) | 306 k | −Y |

## Cycle de modification

1. Ouvrir `moto-N/moto.blend` dans Blender et modifier.
2. `blender -b -P tools/moto/normalize.py -- assets/moto-src/moto-N <rotation>` (ré-exporte `moto_raw.glb`).
3. `bash tools/moto/build-glb.sh N` régénère `assets/models/motos/moto-N-high.glb` et `moto-N-low.glb` (versionnés).

## Reconstruire un `.blend` depuis le FBX

```bash
blender -b -P tools/moto/fbx_to_raw.py    -- assets/moto-src/moto-N <fichier>.fbx   # import
blender -b -P tools/moto/link_textures.py -- assets/moto-src/moto-N                 # textures -> matériaux
blender -b -P tools/moto/normalize.py     -- assets/moto-src/moto-N <0 ou 90>       # échelle, avant, origine
```

(`90` pour la moto 1 dont l'avant est sur −X, `0` pour les deux autres.)

Repère final : mètres, +Y haut, la moto regarde +Z, son côté gauche est +X, origine au sol au centre de l'emprise,
2,2 m de long (comme la voiture, voir `docs/car/CYBERCAR.md` §3).
