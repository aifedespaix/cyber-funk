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
3. `bash tools/car/build-glb.sh` régénère `assets/models/car/car-cyber-high.glb` et `car-cyber-low.glb` (versionnés).
4. Relancer les tests de `cf-world-neon` (le lecteur GLB et le cadre de la voiture les valident).

Doc du modèle (nœuds, matériaux) : `docs/car/CYBERCAR.md`. Les textures 4k (≈ 90 Mo) ne sont pas copiées.

## La voiture actuelle : la « dirty car » (volant à gauche)

Les deux voitures sont livrées et l'opérateur choisit laquelle est à l'écran : `car-dirty-*.glb` (dirty car, par défaut) et `car-cyber-*.glb` (CyberCar, `build-glb.sh`).

1. Modifier `DirtyCar_Game.blend` (objets découpés, pivots `steer_*` / `spin_*`, matériaux nommés par rôle).
2. Exporter en `assets/car-src/dirty_raw.glb` (glTF 2.0, +Y haut, matériaux).
3. `bash tools/car/build-dirty-glb.sh` régénère `car-dirty-high.glb` et `car-dirty-low.glb` ; le masque de peinture est
   `dirty_bake/paint_mask.png`, la couleur déjà teintée violet `dirty_bake/body_color.png`.
4. Relancer les tests de `cf-world-neon`.

`DirtyCar.blend` : import brut (volant à droite) ; `DirtyCar_LHD.blend` : volant à gauche, avant la découpe.

## Kit carrosserie (réactions au beat)

Matériaux ajoutés à `DirtyCar_Game.blend`, lus par rôle (`MaterialRole`, nom du matériau) :
`neon_beat` / `neon_beat_alt` (lueur et teinte secondaire, battent sur le kick), `light_bar` (bandeau arrière : feu stop +
kick), `light_sensor` (grille du phare droit). Les pièces statiques sont fusionnées **par matériau** (`kit_*`) : une
entité Bevy par matériau, pas par pièce. Pivots animés en Rust : `spin_fan_roof` (ventilateur), `spin_holo` (hologramme).
Tout passe par le pouls déjà borné par le limiteur de flashs (règle 7).
