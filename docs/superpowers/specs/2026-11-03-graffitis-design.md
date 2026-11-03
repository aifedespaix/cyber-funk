# Graffitis et hologrammes de toit (conception)

Chantier 3.3 sur 4 du thème « rendre le monde néon moins propre » (après les façades vivantes et le moteur de
quartiers 3.1, voir `2026-11-01-quartiers-moteur-design.md`). Les bâtiments et le mobilier par quartier (3.2) et les
transitions spectaculaires (3.4) restent séparés.

## Intention

Le quartier « banlieue » du moteur de quartiers est annoncé comme « banlieue graffiti », mais rien n'y est peint pour
l'instant : seules la palette, la densité et les hauteurs changent. Ce chantier donne à la ville du **vécu** : des
tags et des bombages sur les murs, plus marqués en banlieue, plus rares ailleurs.

Un second registre prolonge l'idée : certains graffitis sont des **hologrammes** : une **barre horizontale** (le
projecteur) qui **projette les lettres en l'air au-dessus d'elle**, à effet néon lumineux (glow). C'est le graffiti qui
passe du mur au ciel, en lumière, propre à l'univers cyberpunk. Ces projecteurs se posent **où l'on veut dans le
décor**, notamment sur les immeubles, en **plusieurs tailles**.

## Ce que l'utilisateur a décidé

- Des graffitis dans la ville, avec le quartier banlieue comme terrain principal.
- **Certains** graffitis (pas tous) sont des **hologrammes** à effet **glow néon** : une barre horizontale qui projette
  le texte au-dessus d'elle. Placement libre dans le décor (surtout sur les immeubles), tailles variées.
- Texte avec des polices (pas d'images ni de formes). Les polices sont dans `assets/font/`, sous CC0 d'après l'utilisateur
  ; `a_another_tag` est une police dessinée par un ami de l'utilisateur.
- Style cyberpunk. Les mots vivent dans **un fichier modifiable** auquel l'utilisateur peut ajouter des entrées.

## Licences des polices

L'utilisateur indique que les polices de `assets/font/` sont sous **CC0** (domaine public) et que `a_another_tag` a été
dessinée par un ami. Elles sont donc versionnables, à une condition de traçabilité :

- `assets/fonts/LICENCES.md` liste chaque police : nom, source (lien), licence, date. Pour `a_another_tag` : nom de
  l'auteur et mention de son accord d'utilisation et de publication dans le dépôt.
- **Écart relevé, à lever par l'utilisateur** : les fichiers de licence DANS les archives disent autre chose. `jraot` et
  `street_wars` portent « démo, usage personnel uniquement, commercial interdit » ; `throwupz` porte « usage personnel
  uniquement, partage interdit » ; `don_graffiti` dit « redistribution interdite ». Ces textes contredisent CC0 ; si
  l'utilisateur a une licence plus large obtenue ailleurs (page de téléchargement, achat), il la note dans
  `LICENCES.md` avec le lien. Le constructeur ne versionne une police que lorsque sa ligne est remplie.
- Les archives `.zip` ne sont pas versionnées telles quelles : on versionne seulement les fichiers de police
  utilisés (`.ttf`/`.otf`), dans `assets/fonts/`, sans les PNG d'aperçu ni les fichiers lisez-moi non utiles.
- Police absente ou ligne de licence manquante : le motif est sauté avec un avertissement, le dépôt démarre quand même.

## Objectifs

1. **Bibliothèque de graffitis** : des *motifs* (un mot ou un tag, une police, un style : plein, contour, extrusion)
   décrits en RON, cuits en textures au chargement (comme l'atlas des enseignes, `signs/bake.rs`, avec `cosmic-text`
   déjà au workspace). Aucune texture dessinée pendant le show (règle 8).
2. **Graffitis muraux** : peints sur les façades. Placement déterministe en fonction de la rangée et du quartier
   (fonction pure comme `Row::generate`, pas d'état par immeuble). Plus denses en banlieue.
3. **Projecteurs holographiques** : une barre horizontale posée sur un support (toit, corniche, mur, mât ou sol) qui
   projette un texte en lumière au-dessus d'elle : faisceau léger de la barre vers les lettres, glow néon, lignes
   de balayage, faible scintillement **borné** (voir Sécurité). Trois classes de taille (petit, moyen, grand), de
   l'enseigne de boutique au slogan géant en haut d'une tour.
4. **Placement libre** : deux sources de projecteurs. (a) automatique : tirés par hachage sur les immeubles de la
   rangée selon le quartier ; (b) **posés à la main** dans un fichier RON de décor (position le long de la route,
   côté, hauteur, taille, mot) pour placer un projecteur précis où l'on veut.
5. **Couleur** : le glow suit la palette de scène (`ScenePalette`) et donc l'étalonnage de quartier ; un graffiti
   mural prend des teintes propres au motif, dans la gamme du quartier.
6. **Réaction au son** : les hologrammes respirent avec l'audio (intensité, basses) via le niveau déjà accordé par
   le limiteur, jamais directement par le signal brut.
7. **Mots modifiables** : la liste des mots est dans `assets/graffiti/words.ron`, lisible et modifiable à la main ;
   chaque mot indique sa police, son style et s'il peut être hologramme, mural ou les deux. Rechargé à l'ouverture
   (avec `--features dev`, rechargement à chaud) ; une entrée invalide est ignorée avec un avertissement lisible.
8. **Debug (F4)** : curseurs de densité murale et holographique, forçage d'un motif, interrupteur par registre
   (mural / hologramme), dans la section Quartiers existante.

## Hors périmètre

- Nouvelles formes de bâtiments, mobilier, lampadaires (3.2) ; transitions (3.4) ; brume et grain (chantier 2).
- Graffitis dessinés à la main ou importés comme images, hologrammes en forme de logo, visage ou créature : seul le
  texte en polices est traité ici (un motif image pourra venir plus tard, le format RON le prévoit par une variante
  `Source`).
- Graffitis sur la route, le sol ou la voiture (les décalques de route existent, `signs/road.rs`).
- Écriture arbitraire par l'opérateur pendant le show.

## Anti-objectifs

- Aucun scintillement ni flash qui contourne le limiteur photosensible (règle 7). Le glow holographique se module
  par `FlashLimiter` comme les enseignes (`signs/flash.rs`) ; fréquence de changement ≤ 3 par seconde.
- Aucune police versionnée sans ligne remplie dans `assets/fonts/LICENCES.md`.
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

1. **Deux registres, un seul format de motif.** Un `GraffitiMotif` (RON) : mot, police (identifiant logique, pas un
   chemin), style (plein, contour, extrusion), registres permis (`mural`, `holo`), teintes suggérées. Les mots et
   leurs réglages vivent dans `assets/graffiti/words.ron` (le fichier que l'utilisateur modifie). Je fournis 20 à 30
   mots cyberpunk de départ (noms de quartier, de réseaux, de hackers : jamais de marque ni de texte offensant).
2. **Cuisson en atlas** au chargement, une seule fois, dans une tâche de fond inscrite au `LoadTracker` : un atlas
   mural (couleur et alpha) et un atlas hologramme (masque monochrome, pour que la couleur vienne de la palette et que
   le glow soit net). Police absente ou non documentée dans `LICENCES.md` : motif sauté, avertissement, jamais d'erreur.
3. **Graffitis muraux : décalques dans le shader de façade.** Chaque immeuble calcule par hachage de sa position
   s'il porte un graffiti, lequel, où (zone basse : rez-de-chaussée et premiers étages), à quelle échelle, avec quelle
   teinte. Module `facade_paint.wesl` importé par `facade.wesl`. Aucune entité en plus, aucune allocation.
4. **Projecteur holographique = deux pièces.** (a) La **barre** : un mince parallélépipède horizontal, légèrement
   émissif, qui tient sur un support. (b) Le **texte** : un panneau vertical additif posé au-dessus de la barre,
   orienté vers la route, qui lit l'atlas hologramme. Entre les deux, un faisceau léger (un dégradé vertical dans le
   même matériau) : le texte semble projeté par la barre. Le tout est une seule entité-parent avec deux enfants, tirée
   d'un pool préallouée.
5. **Trois classes de taille** : petit (barre ~2 m, lettres ~1 m, pour devantures et mâts), moyen (barre ~6 m, lettres
   ~3 m), grand (barre ~20 m, lettres ~10 m, pour le haut des tours). Valeurs de départ, à régler à l'œil.
   Un grand projecteur n'est jamais placé sous une hauteur minimale, pour rester lisible depuis la route.
6. **Deux sources de placement, mêmes règles de pool.**
   - Automatique : hachage de la position de l'immeuble selon le quartier, choix de la classe de taille selon la
     hauteur de l'immeuble (les tours portent les grands).
   - Manuel : `assets/graffiti/placements.ron`, une liste `{ distance le long de la route, côté, hauteur, taille,
     mot, rotation }` ; permet de poser un projecteur précis à un endroit précis, y compris sur le sol ou un mât.
     Les placements manuels sont prioritaires sur les automatiques voisins (un projecteur auto qui chevauche est
     retiré). Le placement manuel reste déterministe : même fichier, même résultat.
7. **Support** : le projecteur automatique est posé sur le toit de l'immeuble choisi (hauteur donnée par `Row::generate`)
   ou en saillie sur une façade. Il ne traverse jamais un autre immeuble (test de collision avec `scenery/query.rs`).
8. **Glow néon** : matériau émissif à valeurs > 1 pour déclencher le bloom existant, plus un halo précuit dans
   l'atlas (version nette et version floue) : pas de second passage plein écran. Teinte = couleur d'accent de la
   palette (donc étalonnage de quartier) ; intensité = respiration audio × niveau accordé par le limiteur.
9. **Animation** : apparition par balayage de la barre vers le haut (≤ 1,5 s), lignes de balayage lentes, instabilité par
   tranches décalées en réutilisant la logique de `glyph.wesl` (≤ 3 changements par seconde, proportionnelle au niveau
   accordé ; à 0 l'hologramme est stable).
10. **Densité par quartier** (valeurs de départ, à régler à l'œil) : mural, banlieue 1 immeuble sur 3, résidentiel 1 sur
    10, centre 1 sur 25 ; projecteur automatique, centre 1 immeuble sur 10, banlieue 1 sur 18, résidentiel quasi nul.
    Plus d'hologrammes au centre qu'en banlieue : ils relèvent du néon, le bombage de la banlieue. Plafond : 12
    projecteurs actifs en même temps, tous types confondus.
11. **Transition de quartier** : même règle que le dégradé (hachage, probabilité croissante) ; rien ne disparaît d'un
    coup dans le champ de vision.
12. **Qualité** : `SceneryConfig::set_quality` retire d'abord les petits projecteurs lointains, puis le halo flou, puis
    les graffitis muraux lointains. Qualité minimale : seuls les projecteurs proches et les graffitis proches restent.

## Choix laissés au constructeur

- Écriture exacte du RON des motifs et nombre de motifs (proposition : 12 à 20, dont 4 à 6 `holo_ok`).
- Résolution et taille des atlas (à mesurer : budget mémoire et temps de cuisson, visé < 1 s en tâche de fond).
- Technique exacte de halo (version floue précuite, ou mip levels forcés) ; réglage du bloom.
- Position verticale précise des graffitis muraux et décollement des hologrammes.
- Quelle police sert par défaut quand le mot n'en précise pas.
- Découpage en modules (`graffiti/` : `motif.rs`, `bake.rs`, `placement.rs`, `holo.rs`, `systems.rs`).

## Sécurité du live

- Tout niveau de glow, scintillement ou apparition passe par le limiteur : `FlashLimiter` accorde un niveau, le
  shader ne l'excède jamais. Un test vérifie qu'aucune variation de luminance d'un hologramme ne dépasse la limite
  du limiteur sur une séquence de référence.
- Panne ou police manquante : le graffiti manque, la scène reste complète. Aucune erreur remontée au show.
- Les hologrammes ne se déclenchent pas par les drops de façon brutale ; ils réagissent à l'intensité lissée.

## Critères d'acceptation

- Le dépôt se construit et ses tests passent même si une police est absente (motif sauté, avertissement).
- Avec les polices présentes, les motifs correspondants sont cuits au chargement ; sans elles, un
  avertissement est journalisé et les motifs sont sautés.
- Chaque police versionnée a sa ligne dans `assets/fonts/LICENCES.md` (test qui compare les deux listes).
- `words.ron` et `placements.ron` : une entrée invalide est ignorée avec un avertissement, les autres fonctionnent.
- Un projecteur manuel placé dans `placements.ron` apparaît au bon endroit, dans les trois tailles.
- Le placement est déterministe : deux exécutions donnent le même graffiti au même endroit.
- Le golden `scenery.json` est inchangé pour le centre néon par défaut.
- La banlieue porte visiblement plus de graffitis muraux que le centre ; le centre porte quelques hologrammes.
- Un projecteur montre une barre qui projette le texte au-dessus d'elle ; il apparaît par balayage, brille en néon (glow visible au bloom), suit la palette et respire avec le
  son.
- Le niveau de luminance d'un hologramme ne dépasse jamais la limite du limiteur (test sur séquence de référence).
- Aucune allocation par image dans les systèmes de graffiti (test du type `tests/signs_no_alloc.rs`).
- F4 : densités, forçage d'un motif, interrupteurs mural/hologramme.
- `cargo fmt`, `clippy -D warnings`, `cargo nextest run --workspace` verts.
- Vérification visuelle et captures dans `docs/captures/` faites par un humain (non réalisable par l'agent).

## Questions restantes

Aucune qui bloque le plan. Points que le constructeur fait confirmer par l'utilisateur à la première capture :
tailles des classes de projecteur, densités, lisibilité des mots depuis la voiture.
