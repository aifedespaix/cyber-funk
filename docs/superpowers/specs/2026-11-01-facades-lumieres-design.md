# Façades vivantes : lumières et fenêtres (conception)

Chantier 1 sur 3 du thème « rendre le monde néon moins propre ». Les chantiers 2 (caméra, grain, brume,
réactivité sonore) et 3 (quartiers, formes de bâtiments, mobilier de rue, textures) auront chacun leur document.

## Intention

Le monde néon paraît « trop clean » (mot de l'utilisateur). Pour les façades, la cause est connue : grille de
fenêtres parfaitement régulière, seulement 6 teintes prismatiques par tour (plus 30 % d'ambre et 10 % de glace),
pas de formes de lumière autres que la fenêtre rectangulaire, aucun lien entre les immeubles et la voiture.
Le but est que les immeubles aient un caractère propre et que la lumière bouge de façon lisible, sans casser la
logique pour un spectateur qui s'attarde sur un détail.

## Ce que l'utilisateur a demandé

- Beaucoup plus de variation dans les lumières des immeubles : camaïeux de couleurs, déplacement, formes.
- Des lumières qui suivent une partie de la voiture.
- Plus de variation sur certaines fenêtres en particulier.
- Cohérence : un spectateur qui regarde un détail doit y trouver une logique, pas du hasard.
- Ambiance néon globale conservée.

## Objectifs

1. **Caractère par immeuble** : chaque tour reçoit un tempérament stable, tiré de sa position (comme aujourd'hui,
   fonction pure de l'index de rangée) : dominante de couleur avec 2 ou 3 nuances voisines, densité de fenêtres
   allumées, et un comportement parmi plusieurs (ex. bureau stable, résidentiel vivant, tour de néons qui pulse).
2. **Formes de lumière variées** : fenêtres pleines, bandeaux horizontaux, colonnes verticales, enseignes de
   façade, écrans qui défilent, étages entiers qui changent de couleur.
3. **Mouvement lisible** : vagues qui montent ou descendent sur un immeuble, bandes qui glissent le long de la rue,
   balayages de projecteur sur les façades.
4. **Suivi de la voiture** : des immeubles s'allument à son passage et gardent une traîne qui s'éteint
   progressivement derrière ; les couleurs de la voiture teintent légèrement les façades proches.
5. **Fenêtres à vie propre** : une petite part des fenêtres a un comportement individuel (télé bleutée qui
   papillote, lumière qui s'éteint puis se rallume, néon défectueux qui grésille).
6. **Cohérence** : le comportement d'un immeuble ou d'une fenêtre est fixe tant qu'on le regarde ; un néon qui
   grésille grésille toujours à sa manière ; les réactions au son suivent la même logique d'un instant à l'autre.

## Hors périmètre

- Nouvelles textures, usure, saleté, graffitis (chantier 3).
- Nouvelles formes de bâtiments, toits, balcons, câbles (chantier 3).
- Quartiers et changements de type de ville (chantier 3).
- Caméra, grain, brume, poussière, profondeur de champ (chantier 2).
- Lampadaires et mobilier de rue (chantier 3).

## Anti-objectifs

- Le résultat ne doit pas virer au clignotement aléatoire partout : toute variation est déterministe et lisible.
- Aucun flash ou forte variation de luminance qui contourne le limiteur photosensible (`cf-show`, règle 7).
- Pas d'allocation par image dans les systèmes chauds, pas de compilation de shader pendant un show
  (règles 4 et 8).
- Pas de coût GPU sensiblement plus élevé sans repli : le mode qualité existant (`SceneryConfig::set_quality`)
  doit pouvoir désactiver les effets les plus chers.

## Contraintes (état actuel du code, relevé en reconnaissance)

- Shader des façades : `assets/shaders/cf-world-neon/facade.wesl` (333 lignes, plus `field.wesl` concaténé ;
  prepass `facade_prepass.wesl`). Au-delà de 400 lignes, découper en modules (règle 11) : prévoir d'extraire les
  nouvelles fonctions dans un fichier séparé.
- Matériau : `crates/cf-world-neon/src/facade/material.rs` (`FacadeMaterial`, `FacadeParams`),
  `facade/systems.rs::update_material`, `FacadeLook`. Matériau entièrement émissif, sans texture.
- Fenêtres : grille de 1,3 m sur 2,4 m (`BAY_WIDTH`, `FLOOR_HEIGHT`), aléa `window_rnd` via `h21`, un étage sur six
  environ éteint, 30 % ambre, 10 % glace. Le masque du cadre devient une teinte moyenne à distance : les nouvelles
  formes doivent aussi se dégrader proprement à distance.
- Génération : `scenery/layout.rs` (`Row::generate(index)`, fonction pure de l'index ; la graine du monde n'est
  pas utilisée pour les immeubles). Un `MeshTag` par pièce porte graine, teinte, `Tier` et étage de la ceinture.
  Le caractère d'immeuble doit se dériver de cette même graine, sans ajouter d'état par image.
- Audio déjà branché : clignotement (mid), charleston (`hat`), vague par kick (`beat`), bande de caisse claire
  (`onset`), `bright`, `win_pulse`, `wet`, panne `power`, flash d'orage. Les nouveaux comportements s'ajoutent à
  ces entrées, ils ne les remplacent pas.
- Voiture : position/vitesse disponibles côté monde (`car/`, `car_sync.rs` côté cinéma) ; la couleur de la voiture
  vient de `car/paint`.
- Palette : `cf-render/src/palette/` (11 rôles `SceneColor`, 6 thèmes). Les camaïeux doivent dériver des rôles de
  palette pour suivre les changements de thème (`PaletteMorph`).
- Crates sans Bevy (`cf-audio`, `cf-analysis`, `cf-control`, `cf-show`) inchangées.

## Décisions de conception déjà prises

- Tout reste dans le shader de façade existant, sans nouvelle texture.
- Variation déterministe : tirée de la graine de l'immeuble ou de la fenêtre, jamais d'un aléa par image.
- Le suivi de la voiture passe par des paramètres du matériau (position et vitesse de la voiture, couleur),
  pas par des lumières dynamiques.

## Choix laissés au constructeur

- Le nombre exact de tempéraments d'immeuble et leur répartition (proposition : 5 à 8).
- La liste précise des formes de lumière, du moment qu'elles couvrent celles citées plus haut.
- Le codage de la traîne de la voiture (ex. distance au point de passage, décroissance exponentielle).
- Le mélange par défaut entre « calme et dense avec quelques éclats » et « chargé et clignotant » : l'utilisateur
  n'a pas tranché, **appel du rédacteur** : calme et dense par défaut, avec les éclats concentrés sur une minorité
  d'immeubles et sur les temps forts musicaux. Réglable par `FacadeLook`/looks `.ron`.
- Les tests : au minimum des tests unitaires sur la dérivation déterministe du caractère d'immeuble (côté Rust) et
  une vérification visuelle avec captures dans `docs/captures/`.
- Les réglages exposés à l'opérateur (curseurs dans l'onglet existant) si utile.

## Précisions après relecture (appels du rédacteur, à vérifier par le constructeur)

Réponses tirées du code (relevé rapide, à reconfirmer en lisant les fichiers cités) :

- **Données par pièce** : le `MeshTag` (32 bits) en utilise 24 : graine 10 bits, teinte 4, type 2, étage du
  projecteur + 1 sur 8 (`scenery/layout.rs`). Il reste 8 bits libres. Le caractère d'immeuble est dérivé de la
  graine de 10 bits par un hachage partagé ; le constructeur choisit si le hachage vit en WGSL avec un miroir Rust
  testé par valeurs de référence, ou si Rust calcule le caractère et le range dans les bits libres. La
  deuxième option est préférable si 10 bits de graine ne suffisent pas à bien différencier deux voisins.
- **Route courbe** : le parcours n'est pas une ligne droite (`route/planner.rs`, `route/pilot.rs`). La traîne
  de la voiture ne doit donc pas supposer un axe unique.
- **Qualité** : l'énumération `QualityMode` (`cf-render/src/quality/systems.rs`) et
  `SceneryConfig::set_quality(level)` existent déjà.
- **Limiteur** : plusieurs modules ont déjà leur propre garde (`beams/safety.rs`, `dropfx/safety.rs`,
  `traffic/flash.rs`, `landmarks/flash.rs`, `meteors/flash.rs`) avec des tests (`dropfx/tests.rs`).

Décisions :

1. **Traîne de la voiture** : une petite mémoire bornée (tampon circulaire de N positions passées, N à fixer,
   proposition 16) envoyée au matériau. La lueur d'une façade est une fonction de sa distance aux points récents
   du chemin, avec décroissance dans le temps. Pas d'état par immeuble.
2. **Limiteur** : toute nouvelle variation rapide (néon qui grésille, écran qui défile, vague de balayage) passe
   par le même mécanisme que les autres modules ; les fenêtres à scintillement sont plafonnées à 3 changements
   par seconde (WCAG 2.3.1) et à une faible proportion de la surface visible. Un test dans le style de
   `dropfx/tests.rs` vérifie ces seuils sur une séquence de référence.
3. **Qualité** : repli par branche uniforme dans le shader (pas de variantes à précompiler, règle 8) ; en qualité
   basse on coupe d'abord la traîne, les écrans qui défilent et les fenêtres à vie propre.
4. **Camaïeux** : décalages de teinte réduits (de l'ordre de ±25°, à régler) autour d'un rôle de palette, calculés
   de façon à suivre `PaletteMorph`. L'ambre (30 %) et la glace (10 %) restent comme accents de base, modulés
   par le tempérament de l'immeuble.
5. **Distance** : toutes les nouvelles formes se dégradent en teinte moyenne, comme le cadre de fenêtre
   aujourd'hui ; le rythme des étages (bandes) reste visible à distance parce qu'il coûte peu.
6. **Réglages** : les nouveaux paramètres de `FacadeLook` sont optionnels avec valeurs par défaut serde, pour que
   les fichiers `assets/looks/niveau-*.ron` existants restent valides. Côté opérateur, seulement deux
   curseurs : intensité du suivi de la voiture et vie des fenêtres.
7. **Mesure de performance** : scène de référence et tolérance à fixer par le constructeur à partir des
   captures et mesures existantes ; à défaut, comparer avant/après sur la même séquence.

## Critères d'acceptation

- Deux immeubles voisins ont visiblement des dominantes de couleur et des comportements différents.
- Au moins 4 formes de lumière différentes sont visibles dans une même vue de ville.
- Au passage de la voiture, les façades proches réagissent avec une traîne qui s'éteint, sans scintillement.
- Une fenêtre à comportement propre se comporte de façon identique d'une seconde à l'autre (même graine, même
  motif).
- Avec le limiteur photosensible actif, aucune séquence de test ne dépasse ses seuils.
- Pas de régression de budget d'image mesurable sur la scène de référence en qualité haute ; un repli existe en
  qualité basse.
- `cargo fmt`, `clippy -D warnings`, `cargo nextest run --workspace` verts.
