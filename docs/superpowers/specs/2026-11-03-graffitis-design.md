# Graffitis et hologrammes de toit (conception)

Chantier 3.3 sur 4 du thème « rendre le monde néon moins propre » (après les façades vivantes et le moteur de
quartiers 3.1, voir `2026-11-01-quartiers-moteur-design.md`). Les bâtiments et le mobilier par quartier (3.2) et les
transitions spectaculaires (3.4) restent séparés.

## Intention

Le quartier « banlieue » du moteur de quartiers est annoncé comme « banlieue graffiti », mais rien n'y est peint pour
l'instant : seules la palette, la densité et les hauteurs changent. Ce chantier donne à la ville du **vécu** : des
tags et des bombages sur les murs, plus marqués en banlieue, plus rares ailleurs.

Un second registre prolonge l'idée : certains graffitis sont des **hologrammes** projetés **au-dessus des immeubles**,
à effet néon lumineux (glow). C'est le graffiti qui passe du mur au ciel : même dessin, mais en lumière, propre à
l'univers cyberpunk.

## Ce que l'utilisateur a décidé

- Des graffitis dans la ville, avec le quartier banlieue comme terrain principal.
- **Certains** graffitis (pas tous) peuvent être affichés en **hologramme en haut des immeubles**, avec un effet
  **glow néon**.
- Les polices de graffiti sont déjà téléchargées dans `assets/font/` (voir « Licences » : elles ne peuvent pas toutes
  être versionnées).

## Licences des polices (relevé : bloquant pour le dépôt)

Le dépôt est sur GitHub (`aifedespaix/cyber-funk`). Lecture des fichiers de licence fournis dans les archives :

| Archive | Licence relevée | Versionnable ? |
| --- | --- | --- |
| `jraot.zip` | démo, **usage personnel uniquement**, commercial interdit | non |
| `street_wars.zip` | démo, **usage personnel uniquement**, commercial interdit | non |
| `throwupz.zip` | **usage personnel uniquement**, interdit en média public sans accord, partage interdit | non |
| `don_graffiti.zip` | achetée : usage commercial permis, **redistribution interdite** | non (publier = redistribuer) |
| `a_another_tag.zip` | aucun fichier de licence | non, tant que la licence est inconnue |

Conséquence, dans l'esprit de la règle 9 de `CLAUDE.md` (musiques) :

- **Aucune de ces polices ni aucun atlas cuit à partir d'elles n'entre dans git.** Elles vivent dans
  `assets/local/fonts/` (déjà ignoré par `.gitignore`) ; `assets/font/` (non suivi) est à déplacer ou à ignorer.
  Un atlas cuit reste une redistribution déguisée de la police.
- Le dépôt versionne **au moins une police à licence libre** (OFL, CC0…) avec sa licence dans
  `assets/fonts/LICENCES.md`, pour que le dépôt se construise et se teste sans les polices privées.
- Si l'app est un jour diffusée (binaire, release), les polices « personnel uniquement » ne doivent pas y figurer :
  c'est un point à trancher par l'utilisateur avant toute publication (voir « Questions pour l'utilisateur »).

## Objectifs

1. **Bibliothèque de graffitis** : des *motifs* (un mot ou un tag, une police, un style : plein, contour, extrusion)
   décrits en RON, cuits en textures au chargement (comme l'atlas des enseignes, `signs/bake.rs`, avec `cosmic-text`
   déjà au workspace). Aucune texture dessinée pendant le show (règle 8).
2. **Graffitis muraux** : peints sur les façades. Placement déterministe en fonction de la rangée et du quartier
   (fonction pure comme `Row::generate`, pas d'état par immeuble). Plus denses en banlieue.
3. **Hologrammes de toit** : un sous-ensemble des motifs, rendu en lumière au-dessus de certains immeubles :
   panneau plat orienté vers la route, léger décollement du toit, glow néon, lignes de balayage, faible
   scintillement **borné** (voir Sécurité).
4. **Couleur** : le glow suit la palette de scène (`ScenePalette`) et donc l'étalonnage de quartier ; un graffiti
   mural prend des teintes propres au motif, dans la gamme du quartier.
5. **Réaction au son** : les hologrammes respirent avec l'audio (intensité, basses) via le niveau déjà accordé par
   le limiteur, jamais directement par le signal brut.
6. **Debug (F4)** : curseurs de densité murale et holographique, forçage d'un motif, interrupteur par registre
   (mural / hologramme), dans la section Quartiers existante.

## Hors périmètre

- Nouvelles formes de bâtiments, mobilier, lampadaires (3.2) ; transitions (3.4) ; brume et grain (chantier 2).
- Graffitis dessinés à la main ou importés comme images : seul le texte en polices est traité ici (un motif image
  pourra venir plus tard, le format RON le prévoit par une variante `Source`).
- Graffitis sur la route, le sol ou la voiture (les décalques de route existent, `signs/road.rs`).
- Écriture arbitraire par l'opérateur pendant le show.

## Anti-objectifs

- Aucun scintillement ni flash qui contourne le limiteur photosensible (règle 7). Le glow holographique se module
  par `FlashLimiter` comme les enseignes (`signs/flash.rs`) ; fréquence de changement ≤ 3 par seconde.
- Aucune police non libre dans git, aucun atlas cuit d'une police non libre dans git.
- Pas d'allocation par image en régime établi (règle 4) ; pas de chargement ni de compilation de shader pendant
  un show (règle 8) ; pas de `NoFrustumCulling` (règle 6).
- Pas de fichier de plus de 400 lignes (règle 11).
- Le golden `tests/golden/scenery.json` ne change pas pour le quartier « centre néon par défaut » : sans graffiti
  configuré, la génération reste identique.
- Le centre néon ne devient pas sale : il reste le quartier propre, avec au plus quelques hologrammes.

## Contraintes (état du code, relevé)

- `cosmic-text` 0.19 (features `std`, `swash`) est déjà utilisé par `signs/` pour cuire du texte en atlas au
  chargement, dans une tâche de fond inscrite au `LoadTracker`. Le graffiti réutilise ce chemin.
- Les façades sont un matériau shader (`facade.wesl` et modules `facade_*.wesl`, 19 Ko pour le principal) ;
  `facade_life.wesl` (lumières vivantes) et `facade_shapes.wesl` existent déjà. Ajouter une couche de peinture au
  shader de façade est possible mais ce fichier est déjà gros : préférer un module `facade_paint.wesl` importé.
- `Row::generate(index, &SceneryConfig)` est pure, une rangée fait 26 m, jusqu'à 36 pavés par rangée, 34 rangées
  vivantes (1 224 entités préallouées). `DistrictLook` (`scenery/districts.rs`) porte déjà les paramètres par type de
  quartier (`height_scale`, `skip_chance`, `temper_weights`) : on y ajoute les densités de graffiti.
- `SceneryConfig.districts` : plan de quartiers, par défaut « centre néon partout » ; c'est le crochet pour que la
  banlieue peigne et pas le centre.
- Les effets de lumière par pièce passent par les couches de rendu (`WorldLayer`) ; le glow passe par le
  post-process existant (bloom) plutôt que par un second passage maison.
- Le shader `glyph.wesl` (mot de l'atlas du panneau ERROR) a déjà la logique de tranches/aberration chromatique
  plafonnée à 3 changements par seconde : modèle à suivre pour l'hologramme.
- Le panneau F4 est découpé en sections (`cf-operator/src/debug_panel*`), avec une structure de persistance typée.

## Décisions de conception (appels du rédacteur, à valider)

1. **Deux registres, un seul format de motif.** Un `GraffitiMotif` (RON) : texte, police (identifiant logique, pas un
   chemin), style, marge, ratio, `holo_ok: bool` (le motif a le droit d'être un hologramme), palette de teintes
   suggérée. Les hologrammes ne sont pas des objets à part : c'est un motif dont l'instance est en mode `Holo`.
2. **Cuisson en atlas unique** par registre : un atlas mural (couleur, alpha) et un atlas hologramme (masque
   monochrome, comme l'atlas des enseignes, pour que la couleur vienne de la palette et que le glow soit propre).
   Cuits au chargement, une seule fois, par les polices présentes. **Police absente = motif sauté** avec un
   avertissement, jamais une erreur : le dépôt sans polices privées doit démarrer.
3. **Graffitis muraux : décalques par pavé, dans le shader de façade.** Chaque pavé (immeuble) calcule par
   hachage de sa position s'il porte un graffiti, lequel, où (zone basse : rez-de-chaussée et premiers étages, plus
   crédible), à quelle échelle et avec quelle teinte. Aucune entité en plus : une couche `facade_paint.wesl` lit
   l'atlas. Aucune allocation.
4. **Hologrammes : un pool d'entités préallouées** (panneaux plats, matériau additif) posées sur les toits d'après
   le plan de rangée. Nombre borné (proposition : 12 simultanés au plus). Un hologramme est rattaché à un
   immeuble par sa position hachée, déterministe ; il suit le streaming comme les arches (`scenery/arches.rs`).
5. **Glow néon** : matériau émissif, valeurs > 1 pour déclencher le bloom existant, plus un halo doux (flou
   précuit dans l'atlas : une version nette + une version floue) pour éviter un second passage plein écran.
   Légère montée de teinte vers la couleur d'accent de la palette ; intensité = respiration audio × niveau accordé
   par le limiteur.
6. **Animation de l'hologramme** : apparition par balayage vertical (≤ 1,5 s), lignes de balayage lentes,
   « instabilité » par tranches décalées en réutilisant la logique de `glyph.wesl` (≤ 3 changements par seconde,
   tout proportionnel au niveau accordé par le limiteur ; à 0 l'hologramme est stable).
7. **Densité par quartier** (valeurs initiales, à régler à l'œil) : mural, banlieue 1 immeuble sur 3, résidentiel
   1 sur 10, centre 1 sur 25 ; hologramme, centre 1 toit sur 10, banlieue 1 sur 18, résidentiel quasi nul.
   Les hologrammes sont plus fréquents au centre qu'en banlieue : choix délibéré, car ils relèvent du néon, pas du
   bombage.
8. **Transition de quartier** : un graffiti suit la règle du dégradé existant (hachage de position, probabilité
   croissante) ; aucun graffiti ne disparaît d'un coup dans le champ de vision.
9. **Lisibilité** : les mots sont choisis dans une liste courte et neutre (noms d'ambiance cyberpunk, pas de
   marques ni de texte offensant). La liste est dans le RON, modifiable par l'utilisateur.
10. **Qualité** : `SceneryConfig::set_quality` retire d'abord les hologrammes lointains puis le halo flou, puis les
    graffitis muraux lointains. En qualité minimale : plus d'hologramme, graffitis muraux au plus proche seulement.

## Choix laissés au constructeur

- Écriture exacte du RON des motifs et nombre de motifs (proposition : 12 à 20, dont 4 à 6 `holo_ok`).
- Résolution et taille des atlas (à mesurer : budget mémoire et temps de cuisson, visé < 1 s en tâche de fond).
- Technique exacte de halo (version floue précuite, ou mip levels forcés) ; réglage du bloom.
- Position verticale précise des graffitis muraux et décollement des hologrammes.
- Choix de la police libre versionnée (OFL) et de son style de repli.
- Découpage en modules (`graffiti/` : `motif.rs`, `bake.rs`, `placement.rs`, `holo.rs`, `systems.rs`).

## Sécurité du live

- Tout niveau de glow, scintillement ou apparition passe par le limiteur : `FlashLimiter` accorde un niveau, le
  shader ne l'excède jamais. Un test vérifie qu'aucune variation de luminance d'un hologramme ne dépasse la limite
  du limiteur sur une séquence de référence.
- Panne ou police manquante : le graffiti manque, la scène reste complète. Aucune erreur remontée au show.
- Les hologrammes ne se déclenchent pas par les drops de façon brutale ; ils réagissent à l'intensité lissée.

## Critères d'acceptation

- Le dépôt se construit et ses tests passent **sans** aucune police de `assets/local/fonts/` (police libre de repli).
- Avec les polices privées présentes, les motifs correspondants sont cuits au chargement ; sans elles, un
  avertissement est journalisé et les motifs sont sautés.
- Aucun fichier de police privée ni atlas dérivé n'est suivi par git (test ou garde dans la CI).
- Le placement est déterministe : deux exécutions donnent le même graffiti au même endroit.
- Le golden `scenery.json` est inchangé pour le centre néon par défaut.
- La banlieue porte visiblement plus de graffitis muraux que le centre ; le centre porte quelques hologrammes.
- Un hologramme apparaît par balayage, brille en néon (glow visible au bloom), suit la palette et respire avec le
  son.
- Le niveau de luminance d'un hologramme ne dépasse jamais la limite du limiteur (test sur séquence de référence).
- Aucune allocation par image dans les systèmes de graffiti (test du type `tests/signs_no_alloc.rs`).
- F4 : densités, forçage d'un motif, interrupteurs mural/hologramme.
- `cargo fmt`, `clippy -D warnings`, `cargo nextest run --workspace` verts.
- Vérification visuelle et captures dans `docs/captures/` faites par un humain (non réalisable par l'agent).

## Questions pour l'utilisateur (changent ce qui est construit)

1. **Diffusion** : l'app sera-t-elle un jour publiée ou jouée en public ? Si oui, les polices « personnel
   uniquement » sont à remplacer par des polices libres ou achetées avec licence adaptée. Mon défaut : elles servent
   en local seulement, et le dépôt livre des polices libres.
2. **`a_another_tag`** : d'où vient cette police ? Sans licence connue, je la traite comme privée.
3. **Mots à afficher** : veux-tu des mots précis (ton nom de scène, celui des DJs, un thème) ? Mon défaut : une liste
   neutre d'ambiance cyberpunk.
4. **Hologrammes** : texte seul comme les graffitis, ou aussi des formes (logo, visage, créature) ? Ce document ne
   traite que le texte ; les formes demanderaient un format image.
