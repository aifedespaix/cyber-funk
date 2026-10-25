# Assets

Inventaire des fichiers repris de Sillage (dépôt `aifedespaix/aifedespaix.com`, commit `fd34481`, chemins relatifs à
`apps/main/`). Toute nouvelle ressource s'ajoute ici avec son origine et sa licence, dans la même PR.

| Fichier | Origine dans Sillage | Licence | Taille |
| --- | --- | --- | --- |
| `controllers/keyboard-sillage.ron` | Transcription de la table `OPERATOR_BINDINGS` de `src/components/sillage/engine/show/operator.ts` (pas un fichier copié) | Celle du projet | 3 Ko |
| `titles/drop-titles.ron` | Transcription de la table `DROP_TITLES` de `src/components/sillage/engine/show/drop-titles.ts` (22 paires FR/EN, pas un fichier copié) | Celle du projet | 3 Ko |
| `music/Snabisch - Night Prowler.mp3` | `public/music/Snabisch - Night Prowler.mp3` | CC0 (voir `music/LICENCES.md`) | 2984 Ko |
| `music/Tozan - Winning the Race.mp3` | `public/music/Tozan - Winning the Race.mp3` | CC0 (voir `music/LICENCES.md`) | 2717 Ko |
| `music/Yongki - Chase.mp3` | `public/music/Yongki - Chase.mp3` | CC0 (voir `music/LICENCES.md`) | 5117 Ko |
| `music/enprogames - Robotic City.mp3` | `public/music/enprogames - Robotic City.mp3` | CC0 (voir `music/LICENCES.md`) | 3751 Ko |
| `models/car/car-high.glb` | `public/models/car/export/car-high.glb` | **À confirmer** (voir plus bas) | 2845 Ko |
| `models/car/car-low.glb` | `public/models/car/export/car-low.glb` | **À confirmer** (voir plus bas) | 1058 Ko |
| `models/crafts/craft_cargoA.glb` | `public/models/crafts/craft_cargoA.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 19 Ko |
| `models/crafts/craft_cargoB.glb` | `public/models/crafts/craft_cargoB.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 26 Ko |
| `models/crafts/craft_miner.glb` | `public/models/crafts/craft_miner.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 26 Ko |
| `models/crafts/craft_racer.glb` | `public/models/crafts/craft_racer.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 19 Ko |
| `models/crafts/craft_speederA.glb` | `public/models/crafts/craft_speederA.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 20 Ko |
| `models/crafts/craft_speederB.glb` | `public/models/crafts/craft_speederB.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 19 Ko |
| `models/crafts/craft_speederC.glb` | `public/models/crafts/craft_speederC.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 20 Ko |
| `models/crafts/craft_speederD.glb` | `public/models/crafts/craft_speederD.glb` | CC0, Kenney Space Kit 2.0 (`LICENSE-kenney-space-kit.txt`) | 22 Ko |
| `textures/planets/gas-azure.jpg` | `public/sillage/planets/gas-azure.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 57 Ko |
| `textures/planets/gas-ember.jpg` | `public/sillage/planets/gas-ember.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 82 Ko |
| `textures/planets/gas-rose.jpg` | `public/sillage/planets/gas-rose.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 44 Ko |
| `textures/planets/gas-teal.jpg` | `public/sillage/planets/gas-teal.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 51 Ko |
| `textures/planets/ice.jpg` | `public/sillage/planets/ice.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 47 Ko |
| `textures/planets/moon.jpg` | `public/sillage/planets/moon.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 17 Ko |
| `textures/planets/ocean.jpg` | `public/sillage/planets/ocean.jpg` | CC0 (voir `textures/planets/LICENCE.md`) | 48 Ko |
| `fonts/DejaVuSans-Bold.ttf` | Aucune (police externe, projet DejaVu 2.37, paquet `fonts-dejavu-core`) ; cuisson des enseignes (J7-01) | Bitstream Vera (voir `fonts/LICENCE.md`) | 693 Ko |
| `fonts/DejaVuSansMono-Bold.ttf` | Aucune (idem) | Bitstream Vera (voir `fonts/LICENCE.md`) | 326 Ko |

Total : ~20 Mo.

## Non repris

- Les 22 autres pistes de `public/music/` : licences non libres ou non documentées (Darude, The Prodigy, bandes
  originales de Hotline Miami…). À déposer, pour un usage personnel, dans `music/local/` (ignoré par git).
- `public/models/race-future.glb` et `public/models/Textures/colormap.png` (Kenney Car Kit, CC0) : seconde voiture du
  mode Jeu, retirée de cyber-funk (une seule voiture, la CyberCar).
- `public/models/car/export/car_raw.glb` (13 Mo) : export brut avant optimisation, inutile à l'exécution.

## À confirmer : licence de la CyberCar

`models/car/car-high.glb` et `car-low.glb` viennent d'un modèle tiers retravaillé dans Blender
(`CyberCar_work.blend`, voir `docs/sillage/CYBERCAR.md`). Ni le dépôt Sillage ni la doc du modèle n'indiquent son
auteur ni sa licence. **Avant toute distribution d'un paquet**, le propriétaire du dépôt doit confirmer que la licence
du modèle d'origine l'autorise, et la noter ici.

## Références (dans `docs/sillage/`)

- `CYBERCAR.md`, `INTEGRATION.md` : structure du modèle (matériaux, masque de peinture, néons), utile pour J5-05.
- `scene.css` : les 6 thèmes de couleurs de la scène (`packages/tokens/src/scene.css` à la racine du dépôt Sillage),
  référence de J4-08.
