# Motos pilotées, sauts d'immeuble à immeuble, motos sur les ponts : conception

## Intention

Les motos du trafic sont aujourd'hui procédurales (`traffic/model/bike.rs`, variante `BIKE`). L'utilisateur a fourni trois
vraies motos (CC0) et deux robots riggés (CC0). Il veut :

1. un pilote (le « manul ») dans la voiture et sur les motos ;
2. que les motos du trafic soient ces trois modèles, avec une teinte de carrosserie variable, sans toucher au métal ;
3. trois à quatre événements « saut » : un gros immeuble à droite de la route, une moto arrive sur son toit et saute
   au-dessus de la route vers un autre immeuble ; **la caméra reste sur la voiture** (le spectateur voit la scène en passant,
   comme du cinéma, sans plan dédié) ;
4. des motos posées, parfois, sur les routes et les ponts qui traversent (`crossroad`).

Non-buts : un pilote animé en direct (volant qui tourne, buste qui penche) ; le robot articulé comme pilote (voir plus bas) ;
remplacer la voiture par une moto.
Anti-but : tout asset chargé ou shader compilé pendant un show (règle 8), toute entité créée en régime établi (règle 4).

## État des lieux (fait sur la branche `task/X-10-motos-pilotes`)

- `assets/models/motos/moto-{1,2,3}-{high,low}.glb` : 2,2 m, +Y haut, avant vers +Z, textures 1k/2k WebP, meshopt.
  Sources et `.blend` de travail dans `assets/moto-src/` (non versionné). Chaîne : `tools/moto/`.
- `assets/models/characters/{manul,robot}.glb` : riggés, T au repos, matériau `body_paint` + masque de peinture dans
  la texture d'occlusion. Le manul porte quatre poses (`pose_car`, `pose_moto1..3`) calculées par `tools/char/pose.py`.
- Le robot articulé ne se pose pas (rig FBX sans contraintes) : hors périmètre tant qu'on n'a pas son `.blend` d'origine.

## Contraintes découvertes dans le code

- `bevy_gltf` ne lit ni `EXT_meshopt_compression` ni `EXT_texture_webp` : le projet a son propre lecteur
  (`car/glb.rs`, `car/model.rs`, `traffic/model/crafts.rs`). Il ne lit que des maillages **statiques** : pas de peau,
  pas d'animation.
- Le trafic dessine ses véhicules par instanciation (`traffic/render`) avec des matériaux « codes » (`kind` par sommet,
  teinte par `MeshTag`) : cela ne convient pas à des motos texturées en PBR. La voiture, elle, a un chemin texturé
  (`StandardMaterial` + `CarPaintExt`).
- Le trafic préalloue ses entités (`POOL` 220, `CAPACITY` par variante, 6 pour la moto) et les événements s'inscrivent dans
  `EventName` / `EventRegistry` (`traffic/names.rs`, `event.rs`). Un événement prend et rend des véhicules à la `Fleet`.

## Décisions

1. **La pose du pilote est figée dans la géométrie à l'export.** `tools/char/pose.py` applique déjà les poses ; un nouveau
   script applique l'armature (maillage déformé, sans peau) et fusionne le pilote dans le GLB du véhicule :
   `moto-N-{high,low}.glb` contient moto + pilote, et `driver-car.glb` est le conducteur à ajouter à la voiture. Le lecteur
   de GLB existant suffit, aucune peau ni animation à porter en Rust. *Conséquence acceptée :* le conducteur ne tourne pas
   les mains avec le volant ; un pilote de moto ne penche pas dans les virages (la moto, elle, penche).
2. **Un matériau de teinte par luminance** (`RiderTintExt`) : `base × (couleur visée / luminance de référence)`,
   multiplié par le masque de peinture quand le matériau en a un (le robot et le manul), appliqué à l'ensemble du
   matériau sinon (les carrosseries des motos). Il marche sur des carrosseries grises ou brunes, là où un décalage de
   teinte ne ferait rien. Métal, rugosité et saleté restent ceux de la texture. Variantes : une couleur par moto à sa
   naissance, tirée dans la palette du look.
   Matériaux à teindre : moto 1 `Carcass1`, `Rear_frame1`, `Rule1` ; moto 2 `przod`, `owiewki` ; moto 3 `cor_base02*`.
3. **Les motos du trafic passent par un petit parc d'entités texturées**, préalloué au démarrage (6 emplacements : les trois
   modèles en double), positionné à chaque image d'après les véhicules `BIKE` de la `Fleet`. Le modèle procédural n'est plus
   dessiné ; la `Fleet`, `LightWalls` et les motifs de `events/bikes.rs` ne changent pas.
4. **Les sauts sont de nouveaux `EventName`** : `Jump` en trois ou quatre variantes (gabarit d'immeuble, sens, hauteur et
   portée tirés), chacun avec une zone dégagée (`Clearing`), un immeuble de départ à droite et un immeuble d'arrivée de
   l'autre côté. Trajectoire balistique calculée à la naissance (pas d'allocation par image), roulis et tangage de la moto
   suivis, un mur de lumière derrière elle. Aucun flash hors du limiteur.
5. **Motos sur les ponts** : `crossroad` pose, parfois, une ou deux motos arrêtées ou lentes sur la route surélevée.
   Mêmes emplacements du parc que les motos du trafic.
6. **Conducteur** : un enfant statique de la voiture (`driver-car.glb`, mis à 1,1 fois la taille du manul), ajouté dans
   `car/spawn.rs`, caché en vue cockpit si elle existe.

## Découpage (une tâche = un lot)

| Lot | Contenu | Dépend de |
| --- | --- | --- |
| X-10-a | Pilote figé : script Blender, `moto-N-*.glb` avec pilote, `driver-car.glb` | rien |
| X-10-b | `RiderTintExt` + shader WGSL + tests, lecture des matériaux de teinte | rien |
| X-10-c | Conducteur dans la voiture | a, b |
| X-10-d | Parc de motos texturées du trafic, remplacement du modèle procédural | a, b |
| X-10-e | Événements « saut » (3 à 4) | d |
| X-10-f | Motos sur les ponts du `crossroad` | d |

## Reste au constructeur

Gabarits exacts des immeubles de saut, paramètres de la balistique, poids de tirage et portes du look des nouveaux
événements, palette de teintes : choix de réglage, à caler en regardant le rendu.

## Vérifications (sans écran dans l'environnement de l'agent)

Tests unitaires de la trajectoire (atterrit sur l'immeuble d'arrivée), du parc (aucune allocation par image), de la
teinte (valeurs du shader reproduites côté CPU) ; l'aspect final est à valider à l'écran par l'utilisateur.

## Animation des motos (X-10-g)

Sans squelette (le lecteur de GLB ne lit que des maillages), on anime des **nœuds entiers** :

- `spin_F`, `spin_R` : roues, séparées des sources par `tools/moto/split_wheels.py` (îlots de géométrie centrés sur l'essieu,
  détachés de leurs parents, transformations appliquées). Pivot vide sur l'essieu, axe X. Le jeu les fait tourner de `v·dt / r`
  par image (pas borné à 0,9 rad pour ne pas paraître tourner à l'envers), `rider::anim`.
- `rider` : pivot vide au bassin, enfant le pilote figé. Il respire à l'arrêt (±0,8°, 6 mm), se couche en arrière quand la moto
  accélère, se penche en freinant, se redresse dans un wheeling (après s'être jeté en arrière à son départ).
- Plongée de la moto entière au freinage (jusqu'à 3°) et « assise » à l'accélération, autour de l'essieu arrière.

**Pourquoi des pivots vides** : `gltf-transform meshopt` range les sommets en entiers et reporte l'échelle (non uniforme) sur le
nœud du maillage, dont l'origine devient le centre de sa boîte. Un pivot vide garde son origine exacte, sans échelle.

Hors périmètre : bras et jambes qui bougent (squelette), roues avant qui tournent au guidon, rayon de pneu lu dans le GLB
(table `WHEEL_DIAMETER_M`, vérifiée par un test).
