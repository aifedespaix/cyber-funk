# Sources de la voiture (non versionnées, sauf ce fichier)

Ce dossier est ignoré par git (≈ 40 Mo). Il contient les sources du modèle de la voiture, venues de
`car-editor` (`C:\Users\clape\Documents\dev\car-editor`).

| Fichier | Rôle |
| --- | --- |
| `CyberCar.blend` | scène Blender de référence |
| `CyberCar_work.blend` | scène de travail (modifiée via le MCP Blender) |
| `car_raw.glb` | export brut de Blender (glTF 2.0, +Y haut, matériaux), entrée du build |
| `2k/` | textures 2k ; `COLOR2Kpng.png` sert à calculer le masque de peinture |

## Cycle de modification

1. Modifier `CyberCar_work.blend` dans Blender.
2. Exporter en `assets/car-src/car_raw.glb` (glTF 2.0, +Y haut, matériaux).
3. `bash tools/car/build-glb.sh` régénère `assets/models/car/car-high.glb` et `car-low.glb` (versionnés).
4. Relancer les tests de `cf-world-neon` (le lecteur GLB et le cadre de la voiture les valident).

Doc du modèle (nœuds, matériaux) : `docs/car/CYBERCAR.md`. Les textures 4k (≈ 90 Mo) ne sont pas copiées.
