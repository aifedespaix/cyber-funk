# Synchronisation de la voiture, décor au repos et nouvelle passation de DJ

Date : 2026-11-09. Suite de `2026-11-08-changement-dj-intro-portique-design.md`. Ce qui est écrit ci-dessous vient de la
demande de l'utilisateur ; les choix de réalisation sont marqués **[mon choix]**.

## 1. Demande

1. **Apparition du véhicule** : plus d'effet « écrasé puis regonflé ». La voiture a déjà sa forme mais elle est un peu
   glitchée, numérique, en mode squelette, et se **synchronise** (Matrice, Animus d'Assassin's Creed).
2. **Immeubles** : ils sortent de terre. Tant qu'ils ne sont pas sortis ils n'ont pas de lumière ; ce sont eux, une fois
   prêts, qui allument leurs fenêtres (et non un réglage global qui les allume).
3. **Passation** : la voiture ralentit, les immeubles se désynchronisent (ils buguent plus), puis se remettent sous terre.
   La route est nue, seule la voiture roule. La caméra passe un peu devant elle ; on voit arriver au loin la voiture du
   DJ suivant. Des variations d'une passation à l'autre. La nouvelle voiture se met côte à côte, l'ancienne se range sur
   le côté pour laisser le centre de la route à la nouvelle. Elles roulent à peu près à la même vitesse avec des
   variations organiques. L'ancienne se désynchronise (étape inverse : squelette, glitch, couleurs et lumières enlevées).
   Une fois disparue, le nouveau DJ est présenté : la ville se synchronise, son panneau s'affiche, la voiture passe
   dessous, c'est parti.
4. **Pause / arrêt** : plus de brouillard, de publicités ni de lasers ; tous les événements désactivés ; plus rien autour.

## 2. Réalisation

### 2.1 Synchronisation (`cf-world-neon::car::sync`)
- `CarControls.presence` (0 absente, 1 synchronisée) déroule le film : squelette (0 → 0,2), carrosserie noire qui reprend
  sa couleur **par paliers** (0,2 → 0,85), néons et phares (0,75 → 1). Extinction = film à l'envers. **[mon choix]**
- Squelette : un calque additif par pièce (`LightKind::Skeleton` dans `car_light.wesl`) : silhouette, arêtes, lignes de
  balayage à hauteur du monde, front qui monte, bandes décalées de 7 cm et blocs d'écran qui tombent (glitch, géométrique,
  règle 7). Il est gonflé de 1,2 cm pour ne pas faire de z-fighting. Il se cache une fois la voiture synchronisée.
- La carrosserie noire passe par l'échelle de valeur de la peinture, pas par un nouveau shader PBR : aucun risque de
  pipeline à compiler pendant un show (règle 8). **[mon choix]**
- Mode calme (`reduce_motion`) : glitch nul.

### 2.2 Décor au repos (`cf_render::SceneIdle`)
- Vrai quand le séquenceur n'est pas en `Running` et que les tours ne sont pas là (`stages.build == 0`).
- Cache la brume et la brume basse, les panneaux de la route, les projecteurs (tours et épinglés), refuse les
  déclencheurs d'événements, et arrête ceux en cours à l'entrée dans l'état. **[mon choix]** : pas d'extinction
  progressive du brouillard, il disparaît au moment où les tours sont entièrement rentrées.

### 2.3 Lumières des tours (`facade.wesl`)
- `awake` : une tour qui sort de terre n'allume ses fenêtres qu'à 12 à 45 m sous la ligne de balayage ; une tour qui se
  désynchronise les éteint dès les premiers 35 % de sa chute. Le front global de fenêtres n'est plus utilisé par la
  séquence (`windows = 1`).

### 2.4 Passation (`run/variant.rs`, `run/handover.rs`, `car/handover_motion.rs`, plan `handover`)
- Variante tirée à chaque passation (hasard déterministe, numéro de la passation et rang du DJ) : côté de l'ancienne,
  vitesse seule (8 à 11,5 m/s), distance d'arrivée (110 à 170 m), délai (8,5 à 10,5 s), durée du côte à côte (3 à 5 s),
  deux variations de vitesse sinusoïdales (± 2 à 7 %), cadrage. Le style de cadrage ne se répète pas deux fois de suite.
- Cinq cadrages : bas et proche, longue focale, rasant, grue, orbite lente.
- Durée : 40 à 52 s. La voiture réelle (pilotée, foyer du streaming) est la nouvelle ; elle est menée par la frise puis
  rendue au pilote 2 s avant le portique.

## 3. Ce qui n'a pas pu être vérifié ici
Pas d'écran ni de GPU dans la session : le rendu (squelette, glitch, lumières des tours, plans de caméra) n'a pas été vu.
Le code WGSL ajouté à `car_light.wesl` a été validé syntaxiquement avec `naga` (avec des doublures pour les imports de
Bevy), le shader des façades n'a pas pu l'être. À regarder en priorité à la première exécution réelle.
