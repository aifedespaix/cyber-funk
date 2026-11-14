# Chantiers visuels : plan de développement fusionné

> Fusion des trois documents de cadrage en **9 chantiers**, à mener dans l'ordre, un par session.
> Le prompt de session est `prompt.md` (racine du dépôt). L'avancement se lit et s'écrit dans le
> **tableau d'état** en bas de ce fichier : c'est la seule mémoire entre deux sessions.
>
> Ce plan est indépendant de `docs/TASKS.md` (tâches J*, tenues par l'orchestrateur) : ne pas y toucher.

Documents sources, cités par leur sigle :

| Sigle | Fichier | Contenu |
|---|---|---|
| **P1** | `docs/worldbuilding/hyper-transit.md` | tunnel, 5 événements, 30 archétypes, 30 pubs/enseignes/graffitis |
| **P2** | `docs/worldbuilding/hyper-transit-phase2.md` | lumière et reflets, réactivité audio, génération, personnages |
| **DA** | `docs/worldbuilding/direction-artistique.md` | constat, palette, plans, post-traitement, rythme, idées de rupture |

---

## Règles communes à tous les chantiers

1. `CLAUDE.md` s'applique en entier (règles 🔴 du §1), **sauf** « une tâche = une branche = une PR » : ces chantiers
   se commitent et se poussent directement sur `main`.
2. **Étendre, ne pas dupliquer.** Avant d'écrire un module, lire celui qui existe sur le même sujet (liste dans
   chaque chantier) et le prolonger. Un second système parallèle (deuxième brouillard, deuxième moteur de
   quartiers, deuxième palette) est un échec du chantier.
3. **Sans état, sans allocation par image** : un effet est `f(graine, t, signaux)` ; uniformes et données
   d'instance de taille fixe ; tout tampon est alloué à l'ouverture (règles 4 et 8).
4. **Limiteur** : toute variation de luminance marquée **[L]** dans les documents passe par le `FlashLimiter` de
   `cf-show`. Un chantier qui ajoute un flash ajoute son test de limiteur.
5. **Qualité adaptative** : chaque effet coûteux a ses trois paliers et se branche sur `QualityLevers`.
6. **Constantes** : celles des documents sont des valeurs de départ. Les regrouper dans une structure de réglage
   par module, pas en nombres épars.
7. **Écart à Sillage assumé** : quand un chantier change un rendu calé sur les références de Sillage
   (`tools/sillage-golden/`, tests de parité), il met à jour le test ou la référence dans le même commit et le dit
   dans le message. Il ne désactive jamais un test pour passer au vert.
8. **Preuve** : tests sans matériel (`proptest`, `insta`, tests de formule), plus au moins une capture (Xvfb et
   rendu logiciel) dans `docs/captures/C<n>/`. Dire dans le journal ce qui n'a pas pu être vérifié à l'image.
9. **Arbitrages** : là où ce plan contredit P1, P2 ou DA, c'est ce plan qui fait foi (il tient compte du code
   existant).

### Arbitrages par rapport aux documents

| Sujet | Document | Décision |
|---|---|---|
| SSR (marche de rayon) | P2 §1.1 | **reporté**. La route garde ses reflets analytiques (`road.wesl` : `lamp_reflection`, `facade_reflection`) et reçoit flaques, rugosité réactive, étirement et aberration. Les couches sont des cibles séparées et le monde n'est pas éclairé : le SSR ne se justifie pas. |
| Lumières groupées, VPL | P2 §1.2 | **non**. Le monde est émissif non éclairé. On garde le halo analytique de façade (`L_halo`) et le bloc d'uniformes `SceneLight` (DA §2.1). |
| Bloom à loi de puissance | P2 §1.2 | pas de nouvelle passe : réglage du `Bloom` de Bevy (DA §3.1). |
| Brouillard en grille ou marche de rayon | P2 §1.3 | **non**. Brouillard analytique par fragment dans une fonction WESL partagée (DA §3.2), qui garde les formules de transmittance, de phase et la modulation audio de P2. |
| 30 archétypes | P1 §2 | composés d'abord à partir du pavé unité existant (`scenery::Part`, un seul lot de dessin). Une primitive nouvelle (cylindre, tore, hyperboloïde) n'est ajoutée que si l'archétype est illisible sans elle, et forme un lot instancié à part. |
| Quartiers A, B, C | P1 §2, P2 §3.2 | ajoutés au moteur de quartiers existant (`scenery/districts.rs`, `districts/`, `district_grades.ron`), pas à côté. |
| Personnages | P2 §4 | il n'y a pas de foule, et le lecteur de GLB ne lit que des maillages statiques. Les 10 tenues s'appliquent aux pilotes et au conducteur existants (`rider/`), les tissus sont des rubans procéduraux ajoutés. |
| Découpage en lots P2-A…P2-I | P2 §5 | remplacé par ce fichier. |

---

## C1 · Signaux et dramaturgie

**But.** Donner à tout le reste des signaux propres : sous-bandes, chaîne de filtrage, matrice de routage, densité
visuelle `Δ`.

- **Sources :** P2 §2.1, §2.2, §2.3 (types et bornes seulement) ; DA §4.2.
- **Existant à lire :** `cf-analysis/src/{features,analyzer,band_gain,texture}.rs`, `cf-show` (sections, intensité,
  limiteur), `cf-render/src/audio/`.
- **Skills :** `realtime-audio`, `show-model`, `rust-conventions`.

**À faire**

- `cf-analysis` : sous-bandes `SB, PB, LM, HM, TR` (biquads pour `SB`/`PB`, FFT pour le reste), flux par bande,
  HFC, spectre 32 bandes log lissé. Champs **ajoutés** à `AudioFeatures`, les anciens inchangés. Rien dans le
  callback `cpal`.
- Une bibliothèque de filtres sans Bevy, réutilisable : attaque/relâchement, crête avec maintien, One-Euro, borne de
  pente, ressort critique et sous-amorti (intégration exacte), bascule de Schmitt, z-score adaptatif.
- Type `Route` et matrice de routage en RON (P2 §2.2), validée au chargement, avec la matrice par défaut qui
  reproduit P2 §2.3. Application côté `cf-render/audio` : une ressource de signaux filtrés que lisent les shaders.
- Densité `Δ` et ses cinq états (DA §4.2), avec fatigue, pré-drop **[L]** et respiration imposée, publiée à côté
  de l'état du show.
- Onde de kick `W` : uniforme commun des 4 derniers horodatages (P1 §0).

**Acceptation**

- [x] Réponse à un échelon identique à 30, 60 et 144 images par seconde (écart < 1 %) pour chaque filtre.
- [x] Sinus à 40 Hz et à 100 Hz : `SB` et `PB` les séparent (rapport > 6 dB). Bruit rose : bandes équilibrées.
- [x] Aucune allocation dans la boucle d'analyse (test ou compteur existant).
- [x] `Δ` sur une piste de démo : instantané `insta` de la courbe, cinq états atteints, pré-drop borné par le limiteur.
- [x] Les shaders existants tournent sans changement visible (les anciens champs sont intacts).

---

## C2 · Image finale

**But.** Une image qui tient ses blancs et ses noirs : exposition, bloom, tonemapping, étalonnage, persistance.

- **Sources :** DA §3.1, §1.1 (paliers d'émissif, contrat d'exposition) ; P2 §1.2 (bloom), §2.3 (aberration, glitch).
- **Existant à lire :** `cf-render/src/postfx/` en entier, `assets/shaders/cf-render/grade.wgsl`,
  `tools/sillage-golden/postfx/README.md`, `quality/levers.rs`.
- **Skills :** `render-performance`, `bevy-patterns`, `live-safety`.

**À faire**

- Bloom : seuil, genou, queue liée à la brume, pulsation inchangée.
- Étalonnage : aberration ciblée (nulle au centre, hautes lumières), halation, grain multiplicatif à hachage
  entier après tonemapping, défilement UV glitché **[L]**.
- Tonemapping : comparaison `KhronosPbrNeutral` / `AgX` (capture côte à côte), décision écrite dans le journal.
- Persistance : tampon d'historique en demi-résolution alloué à l'ouverture, masque de luminance, fuite vers le
  point de fuite, remise à zéro à chaque coupe ; le flash du spectacle s'ajoute après.
- Rayons en espace écran (DA §3.2) sur la cible du bloom, coupés aux paliers bas.
- Outil de mesure sur capture : moyenne `Ȳ`, part du bloom, part de `ice`, histogramme de teintes (DA §1.1). Il
  sert de test à C2, C3 et C4.

**Acceptation**

- [~] Sur la scène de référence : `Ȳ` et part du bloom dans les bornes de DA §3.1. (tenu sur la scène démo seulement, pas sur la ville)
- [x] Grain : noirs inchangés (pixel à 0 reste à 0), aucune fonction `sin` dans le hachage.
- [x] Aberration nulle dans le disque central de rayon 0,35.
- [~] Persistance : aucun flash prolongé (test du limiteur), historique vidé à la coupe. (logique testée ; pas de test GPU de la remise à zéro)
- [~] Captures avant/après dans `docs/captures/C2/` (faites) ; référence de parité Sillage non régénérée (bun/Playwright absents).

---

## C3 · Atmosphère et profondeur

**But.** De l'air entre les plans : brouillard par fragment, éclairage de scène écrit, poussières.

- **Sources :** DA §1.2, §2.1, §3.2 ; P2 §1.3.
- **Existant à lire :** `cf-world-neon/src/weather/` en entier, `weather_{mist,haze}.wesl`, `facade.wesl`, `sign.wesl`,
  `sky_*.wesl`, `cf-render/src/layers/`.
- **Skills :** `render-performance`, `bevy-patterns`, `sillage-porting` (les shaders sont en WESL).
- **Dépend de :** C1 (signaux), C2 (outil de mesure).

**À faire**

- Bloc d'uniformes `SceneLight` (principale, remplissage, contre-jour, brume), rempli depuis la palette, la météo
  et `Δ`.
- Fonction WESL partagée d'atmosphère : transmittance en hauteur, phase de Henyey-Greenstein, teinte de Rayleigh,
  4 tubes proches en forme close. Importée par les shaders de façade, d'enseigne, de trafic, de repères, de route.
- Modulation audio de P2 §1.3 : densité, anisotropie, coquille de kick, bouffée de drop **[L]**, apnée.
- Les cinq plans de DA §1.2 : séparation de valeur, contre-jour par la brume, désaturation au loin.
- Poussières : trois coquilles, un maillage instancié, sans état.
- Distance utile `d_brume` publiée pour le culling (utilisée en C7).

**Acceptation**

- [x] Test de formule : `T` analytique contre intégration numérique (écart < 1 %), limite `u → 0` stable.
- [~] Sur capture : luminance moyenne des plans P1, P2, P3 séparée d'au moins 0,12 ; P1 plus sombre que la brume. (tenu sur la formule, pas sur la capture : les corps de façade actuels sont trop clairs, voir le journal ; à rejuger après C4)
- [~] Aucune passe plein écran ajoutée (tenu, testé) ; coût mesuré avant/après aux trois paliers (non mesuré : rendu logiciel).
- [x] Captures `docs/captures/C3/` : calme, sommet, coquille de kick.

---

## C4 · Sol, façades et matériaux

**But.** Des surfaces qui ont une matière : sol mouillé, façades hiérarchisées, néons à cœur blanc.

- **Sources :** DA §1.1 (fenêtres, paliers), §2.1, §2.2 ; P2 §1.1 (sans le SSR), §1.2 (halo) ; P1 « Maths de
  structure » (4) façade égaliseur.
- **Existant à lire :** `cf-world-neon/src/road/`, `road.wesl`, `facade/`, `facade*.wesl`, `signs/`, `sign.wesl`,
  `cf-render/src/tuning.rs`.
- **Skills :** `render-performance`, `sillage-porting`.
- **Dépend de :** C1, C3.

**À faire**

- Route : flaques (`m_p`), rugosité réactive, ondes de pluie et de kick, étirement vertical et aberration des
  reflets analytiques, grain d'asphalte, flaque de lumière sous le sujet.
- Façades : ombrage par face, occlusion verticale, liseré de contre-jour, micro-détail (coulures, joints), halo
  des 4 sockets proches, fenêtres hiérarchisées (fraction pilotée par `Δ`, intensité log-normale, groupes par
  étage), vu-mètre sur le spectre lissé.
- Néons et enseignes : profil cœur blanc / halo saturé, paliers E1–E4, saturation liée à `HM`.
- Idée de rupture n° 1, **fenêtres habitées** (DA §5), avec repli en aplat au loin.

**Acceptation**

- [ ] Critère de palette de DA §1.1 sur capture (70 % des émissifs dans la dominante ou l'accent, `ice` < 2 %).
- [x] Une fenêtre ne dépasse jamais `0,9·θ_b` (test du shader ou de la fonction miroir en Rust).
- [x] Toujours un seul lot de dessin pour les pavés de la ville. (aucun matériau ni maillage ajouté : un champ d'uniformes de plus)
- [x] Captures `docs/captures/C4/` : sec, mouillé, kick, calme contre sommet.

---

## C5 · Ville procédurale : distribution et archétypes

**But.** Trois quartiers nouveaux et trente archétypes, générés sans répétition.

- **Sources :** P1 §2 (table des archétypes, maths de structure 1 à 3) ; P2 §3.1, §3.2, §3.4 (données d'instance).
- **Existant à lire :** `cf-world-neon/src/scenery/` en entier, `districts/`, `rng.rs`, `cf-render/src/streaming/`,
  `palette/grade_library.rs`, `docs/superpowers/specs/2026-11-01-quartiers-moteur-design.md`.
- **Skills :** `render-performance`, `bevy-patterns`, `show-model`.

**À faire**

- Hachage et hiérarchie de graines (P2 §3.1), branchés sur `rng.rs`.
- Quartiers A (Monolithes), B (Ruche), C (Fonderie) dans le moteur de quartiers existant, avec leur étalonnage.
- Distribution : Voronoï sur grille perturbée, canaux de circulation, présence, hauteur en amphithéâtre, règle de
  contraste, choix d'archétype `π_q[(i + 3j + o_q) mod 10]`, orientation.
- Les 30 archétypes, composés de pavés (voir arbitrages), avec leurs sockets déclarés (types de P2 §3.3) mais
  pas encore habillés.
- Déformations au vertex : bruit 3D, onde de kick par étage, superquadrique et torsion, `Aabb` élargi.
- Variations par instance (proportions, paramètres de forme, palette, usure, phase audio).

**Acceptation**

- [ ] `proptest` : aucun archétype répété dans un voisinage 5×5 ; une cellule régénérée seule est identique.
- [ ] `proptest` : aucune emprise ne coupe un canal de circulation.
- [ ] `insta` : un tronçon à graine fixe par quartier.
- [ ] Aucune création d'entité en régime établi (réserves du streaming) ; aucun `NoFrustumCulling`.
- [ ] Captures `docs/captures/C5/` : une vue par quartier.

---

## C6 · Ville procédurale : habillage

**But.** Ce qui fait qu'aucun bâtiment ne ressemble à un autre : pubs, enseignes, graffitis, câbles, antennes, vapeur.

- **Sources :** P1 §3 (30 visuels, shader hologramme, motifs 16 pas, graffitis UV) ; P2 §3.3 ; P2 §2.3 (câbles,
  pylônes, enseignes).
- **Existant à lire :** `cf-world-neon/src/{signs,graffiti,gantry,beams}/`, `scenery/graffiti_plan.rs`,
  `graffiti_holo.wesl`, `sign.wesl`, `docs/superpowers/specs/2026-11-03-graffitis-design.md`.
- **Skills :** `render-performance`, `sillage-porting`, `live-safety`.
- **Dépend de :** C5 (sockets), C1.

**À faire**

- Occupation des sockets (score de visibilité, probabilités par quartier, plafond par palier).
- Tirage sans doublon `(a0 + n·pas) mod 30` et ses contraintes.
- Les 30 visuels : 10 hologrammes, 10 enseignes à motif 16 pas, 10 graffitis UV. Étendre les systèmes d'enseignes
  et de graffitis existants.
- Câbles en caténaire exacte (résolution de Newton à la génération, courbe et balancement au vertex), règles
  d'appariement et de dégagement.
- Antennes (disque de Poisson, balises **[L]**), conduites de vapeur (chemin de Manhattan, évents sur réserve).

**Acceptation**

- [ ] `proptest` caténaire : les deux ancres sont atteintes, la longueur vaut `L` (écart < 0,1 %), dégagement tenu.
- [ ] `proptest` : aucun visuel répété sur un bâtiment de moins de 30 sockets.
- [ ] Motifs d'enseigne et balises : test du limiteur.
- [ ] Budgets de P2 §3.4 tenus aux trois paliers (compteurs dans le journal).
- [ ] Captures `docs/captures/C6/` : planche des 30 visuels, une rue du quartier B avec ses câbles.

---

## C7 · Quartier tunnel « Hyper-Transit », événements, LOD et culling

**But.** Le quartier signature, et les garde-fous de coût pour toute la ville.

- **Sources :** P1 §1 en entier (intérieur, extérieur, 5 événements) ; P2 §3.4 (LOD, culling, budgets).
- **Existant à lire :** `cf-world-neon/src/{route,road,landmarks,sky,dropfx}/`, `cf-render/src/{culling,streaming,quality}/`,
  `events_catalog.rs`, `active_events.rs`.
- **Skills :** `render-performance`, `show-model`, `live-safety`, `bevy-patterns`.
- **Dépend de :** C1, C3, C5.

**À faire**

- Tunnel intérieur (anneaux, parois) et extérieur (structures suspendues, pylônes à arcs, générateur plasma,
  horizon-spectre, soleil à bandes, grille de sol), comme quartier du moteur existant.
- Les cinq événements EV1–EV5 dans le catalogue d'événements existant : bascule de Schmitt, quantification,
  cooldown, limiteur.
- Idée de rupture n° 2, **ville-spectrogramme** (DA §5), sur l'horizon.
- LOD par taille projetée avec hystérésis et fondu tramé ; culling de brume, de courbure du tunnel et de bouche ;
  silhouettes d'horizon fusionnées par tronçon.

**Acceptation**

- [ ] Chaque événement : test de déclenchement, de cooldown et de limiteur sur signaux synthétiques.
- [ ] Test de formule de la portée de vue en tunnel courbe ; aucune entité extérieure active au milieu du tunnel.
- [ ] Budgets de P2 §3.4 tenus ; aucun chargement d'asset ni compilation de shader pendant le show (préchargement).
- [ ] Captures `docs/captures/C7/` : intérieur, bouche, extérieur, un événement.

---

## C8 · Personnages et tenues

**But.** Dix tenues lisibles de loin, coutures lumineuses et tissus au rythme.

- **Sources :** P2 §4 en entier.
- **Existant à lire :** `cf-world-neon/src/rider/`, `rider_tint.wesl`, `tools/{char,moto}/`,
  `docs/superpowers/specs/2026-11-01-motos-pilotes-design.md`.
- **Skills :** `render-performance`, `sillage-porting`.
- **Dépend de :** C1.

**À faire**

- Dix styles comme variantes du matériau des pilotes et du conducteur : table de paramètres, tirage par graine.
- Coutures lumineuses : abscisse curviligne procédurale (bandes le long du corps si le maillage n'a pas de second
  jeu d'UV), cinq termes de P2 §4.1, luminance bornée.
- Tissus : rubans et pans procéduraux attachés au pilote, onde au vertex ; cape Verlet à tampons fixes pour le
  pilote au premier plan seulement.

**Acceptation**

- [ ] Onde de tissu : normale analytique vérifiée contre différences finies ; anti-pénétration tenue.
- [ ] Verlet : stable sur 10 minutes simulées à cadence variable, aucun point au-delà de la longueur de la cape.
- [ ] Dix styles distincts sur une planche (`docs/captures/C8/`) ; aucun chargement en show.

---

## C9 · Mise en scène et signatures

**But.** La caméra et les trois dernières idées de rupture, puis la passe finale de cohérence.

- **Sources :** DA §4.1, §5 (idées 3, 4, 5) ; P2 §2.3 (tremblement, coup de focale, pulsation d'échelle).
- **Existant à lire :** `cf-cinema/src/` en entier, `cf-render/src/layers/rig.rs`, `postfx/`.
- **Skills :** `bevy-patterns`, `render-performance`, `live-safety`, `show-model`.
- **Dépend de :** C1, C2, C3.

**À faire**

- Caméra : bornes de vitesse et d'accélération, lissages par ressort, courbes d'entrée et de sortie, tremblement
  par `trauma`, coup de focale, règle « un mouvement à la fois ».
- Budget de simultanéité et règle « une idée par phrase » (DA §4.2), appuyés sur la fraîcheur de `cf-show`.
- **Balayage temporel** (anneau d'images alloué à l'ouverture), **pli du monde** (fonction au vertex commune,
  frustum élargi), **colosse holographique**.
- Passe de cohérence : relire le tableau de constat de DA §0, vérifier chaque ligne sur capture, noter ce qui reste.

**Acceptation**

- [ ] Caméra : vitesse et accélération angulaires bornées sur une session simulée de 10 minutes (hors coupes).
- [ ] Pli du monde : aucun objet visible supprimé par le culling à `κ_max` ; aucun `NoFrustumCulling`.
- [ ] Balayage temporel : mémoire constante, coupé à chaque coupe de plan.
- [ ] Captures `docs/captures/C9/` : les trois idées, et le tableau de constat DA §0 rempli dans le journal.

---

## Tableau d'état

Statuts : ⬜ à faire · 🟨 en cours (le journal dit où reprendre) · ✅ terminé.

| # | Chantier | Statut | Dernier commit | Reste à faire / à vérifier sur le matériel cible |
|---|---|---|---|---|
| C1 | Signaux et dramaturgie | ✅ | voir `git log` (dernier commit « C1 ») | Sur le matériel cible : seuils de `z` des transitoires et étalonnage `FFT_TO_RMS` sur une vraie entrée ligne/micro ; ressenti du creux d'avant-drop. Routage non encore lu par les shaders (C2 à C4). |
| C2 | Image finale | ✅ | voir `git log` (« C2 ») | Critères `[~]` : Ȳ et bloom mesurés sur la démo, pas sur la ville (à refaire en C3/C4) ; référence de parité Sillage non régénérée (bun/Playwright absents) ; remise à zéro GPU de la persistance non testée ; rayons à valider à l'œil. Matériel cible : cadence, bloom fin, couleurs sur projecteur. |
| C3 | Atmosphère et profondeur | ✅ | voir `git log` (« C3 ») | Critères `[~]` : séparation des plans à rejuger sur capture après C4 (corps de façade trop clairs aujourd'hui) ; coût GPU aux trois paliers non mesuré. Matériel cible : densité `σ_t·ρ` et luminance de la brume sur projecteur, lisibilité de la coquille de kick, taille des poussières. `FogReach` publié mais pas encore lu par le culling (C7) ; `AtmosphereLook::apnea` pas encore écrit (EV4, C7) ; `SceneLight` (principale, remplissage, contre-jour) publié mais l'ombrage des faces est pour C4. |
| C4 | Sol, façades et matériaux | 🟨 | voir `git log` (« C4 ») | Fait : relief des façades, fenêtres hiérarchisées et plafonnées, grain, flaque de lumière et ondes de kick de la route. Reste : rugosité réactive, reflet des fenêtres aligné, micro-détail et halo des façades, vu-mètre, néons à cœur blanc, fenêtres habitées, critère de palette. Voir le journal (point de reprise numéroté). |
| C5 | Ville : distribution et archétypes | ⬜ | — | — |
| C6 | Ville : habillage | ⬜ | — | — |
| C7 | Tunnel, événements, LOD et culling | ⬜ | — | — |
| C8 | Personnages et tenues | ⬜ | — | — |
| C9 | Mise en scène et signatures | ⬜ | — | — |

## Journal

Une entrée par session, la plus récente en haut : date, chantier, ce qui est fait, décisions prises, critères
non tenus et pourquoi, point de reprise si le chantier reste 🟨.

### 2026-11-14 · C4 Sol, façades et matériaux · 🟨

**Fait (façades, `facade.wesl`, actif seulement quand le bloc `SceneLight` est rempli)**
- Relief sans ombre portée : ombrage par face `0,45 + 0,55·(n·dir_key)⁺`, occlusion verticale du pied (`0,35` → 1 sur le premier
  quart de la hauteur), remplissage hémisphérique par `c_fill`, liseré de contre-jour `(1 − n·v)⁴·(dir_rim·−v)⁺`.
- Fenêtres hiérarchisées : part des étages allumés `0,18 + 0,42·Δ` (plus le biais du tempérament), allumage **par étages
  entiers** (0,9 dans un étage allumé, 0,03 ailleurs), intensité log-normale `E1·exp(0,6·n)`, plafond `0,9·θ_b` par canal quoi
  que fasse la musique. Miroir Rust `facade::windows` (constantes comparées au shader par un test).
- `SceneLight` porte maintenant `Δ` (`c_fill.w`) et le plafond des fenêtres (`c_key.w`, lu de `BLOOM_THRESHOLD`).

**Fait (route, `road.wesl`, même condition)**
- Grain d'asphalte fondu dans sa moyenne dès qu'une cellule passe sous deux pixels (`fwidth`) : plus de scintillement au loin.
- Flaque de lumière sous le sujet : `c_key·0,12·exp(−r²/σ²)`, σ = 6 m, centrée sur `scene.source`.
- Ondes de kick sur l'eau : pente radiale ajoutée à la surface mouillée (14 m/s, 0,9 m, 0,7 s), à plat en respiration ou flashs
  coupés (`scene.shell.w`). Flaques, rides de pluie, rugosité et étirement des reflets existaient déjà (portage de Sillage).

**Mesures (city_demo, rendu logiciel, sans post-traitement, luminance linéaire) : avant C3 → C3 → C4**
- Calme : Ȳ 0,073 → 0,075 → **0,061** (cible 0,06 de DA §3.1) ; pixels émissifs (`Y > 0,2`) 10,2 % → 6,8 %.
- Sommet : Ȳ 0,097 → 0,100 → 0,089 ; l'écart sommet/calme passe de +33 % à +46 %.
- `Y > 0,8` : 0,36 % au calme, 1,55 % au sommet (< 2 %).
- Captures `docs/captures/C4/` : sec, calme, sommet, kick, mouillé.

**Décisions**
- Le plafond porte sur le canal le plus fort (pas sur la luminance) : c'est ce que le préfiltre du bloom de Bevy compare au seuil.
- Rien ne change bloc à zéro (tests de parité et démos sans `AtmospherePlugin`) : aucune référence Sillage à régénérer.
- Albédo du béton 0,18 (milieu de la plage 0,12 à 0,25 de DA §2.2).

**Reste à faire (point de reprise)**
1. Route (`road.wesl`, `road/`) : rugosité réactive (`Target::Puddles`, pas encore dans le bloc), aberration des reflets
   analytiques, et `facade_reflection` qui dessine encore une fenêtre sur deux (à aligner sur `window_share`, sinon le reflet
   montre plus de fenêtres que la façade). Pas de miroir Rust ni de test pour `kick_ripples` et la flaque de lumière.
2. Façades : micro-détail (coulures, joints, effacé par `fwidth`), halo des 4 sockets proches, vu-mètre sur
   `FilteredSignals::spectrum` (à ajouter au bloc ou à `FacadeParams`), chanfrein lumineux de toit sur 1 immeuble sur 3.
3. Néons et enseignes (`sign.wesl`, arêtes de `facade.wesl`) : profil cœur blanc / halo saturé, paliers E1 à E4, saturation liée à `HM`.
4. Fenêtres habitées (DA §5, idée 1) avec repli en aplat au loin.
5. Critère de palette (70 % des émissifs dans la dominante ou l'accent) avec `cf_render::postfx::measure` sur les captures.
6. Le plafond couvre la fenêtre et la traîne de la voiture (charleston et pouls de la ville passent par `lit`, donc dedans).
   La bande de caisse claire et la ceinture lumineuse ne sont pas des fenêtres : à ranger dans les paliers E2 avec les néons (point 3).

### 2026-11-14 · C3 Atmosphère et profondeur · ✅

**Fait**
- `cf-world-neon::atmosphere` (nouveau module, `AtmospherePlugin` ajouté par `NeonWorldPlugin`) :
  - `formula` : épaisseur optique analytique, phase de Henyey-Greenstein, tube en forme close, coquille de kick, `d_brume`,
    perspective atmosphérique. Miroir Rust de `atmosphere.wesl`, un test compare les constantes des deux textes.
  - `modulation` : densité `ρ0 = ρ_look·(0,6 + 0,8·E_s)` lissée, anisotropie 0,55 → 0,85, bouffée de drop **[L]**, apnée en une
    mesure, vent, pluie et humidité. État de taille fixe.
  - `light` : bloc `SceneLight` (principale, remplissage, contre-jour, brume, Rayleigh, 4 kicks, 4 tubes), composé depuis la
    palette, la teinte d'ambiance et `Δ` ; rapport principale/remplissage 8:1 → 16:1.
  - `dust` : trois coquilles (200/800/1500, 100/300/500, 0/100/0 selon `QualityLevers::particle_density`), un maillage, un
    draw call, tout au vertex shader, `Aabb` explicite.
- `atmosphere.wesl` ajouté au bout des shaders de façade, route, enseigne, véhicule de trafic et repères (`@binding(9)`),
  appelé par `atmo_fog` / `atmo_fade`. Aucune passe plein écran, aucune texture lue.
- `FogReach` (distance utile de la brume, `T < 2 %`) publié pour le culling de C7 ; `AtmosphereLook::apnea` pour EV4.
- `city_demo` : trois scènes d'atmosphère, `CF_ATMO=0` pour la capture « avant ».

**Décisions / écarts**
- Le fondu de distance de chaque shader (vers sa teinte de fond, presque noire) est **gardé** : il efface le détail au loin et
  laisse la silhouette, que la brume découpe ensuite. Bloc à zéro (tests, démos sans le plugin) : rendu d'avant, à l'identique,
  donc aucune référence de parité Sillage à régénérer.
- Phase de Henyey-Greenstein **ramenée à un pic de 1** : la forme brute a un pic ×11 entre `g = 0,55` et `0,85`, moduler `g` par
  le son aurait été un flash. Monter `g` resserre le lobe sans relever son sommet.
- Bouffée de drop : enveloppe d'un `FlashLimiter` (voie `Sweep`) propre à la brume, pas un ADSR libre ; pic ×1,8, montée en un
  demi-temps au moins. Coquille de kick et scintillement coupés en respiration ou flashs coupés.
- Scintillement des poussières lointaines : 3 Hz fondu (jamais de saut) au lieu des 8 Hz de DA §3.2 (règle 7 et test des motifs ≤ 3 Hz).
- `(1 − e^{−u})/u` : développement `1 − u/2` sous `|u| < 10⁻³`, `1/u` au-delà de 30, `u` borné à −30 (pas de débordement).
- Le contre-jour est dans l'axe de la route et s'ajoute à ce que la brume diffuse : c'est lui qui éclaircit le point de fuite.
- Tubes proches : le sujet et les trois premières lumières des immeubles-vitrines (`CityField`), rayon minimal 1,5 m, gain 4
  (à 20 et 0,5 m le sujet délavait toute la rue : vu sur capture, corrigé).
- `σ_t·ρ_look = 0,0037 /m` : `1 − T` = 0,2 à 60 m, 0,89 à 600 m (plans de DA §1.2).
- Hors périmètre, corrigés au passage : `signs::debug::publish_catalog` exigeait `SignCatalog` (absent des démos du monde seul,
  `city_demo` plantait au démarrage) ; lints clippy apparus avec Rust 1.100 (`map_or_default`, boucles indexées) dans
  `cf-audio`, `cf-debuglog`, `cf-show`, `cf-render`, `cf-operator`, `cf-world-neon`.

**Mesures (city_demo, rendu logiciel, sans post-traitement), luminance linéaire avant → après**
- Départ : image 0,073 → 0,076 ; bande médiane 0,121 → 0,131 ; autour du point de fuite 0,189 → 0,197.
- Calme 0,075, sommet 0,100, pluie 0,112.
- L'effet est réel mais discret, et la séparation des plans de 0,12 **n'est pas atteinte à l'image** : les corps de façade
  proches sont à 0,16 de luminance (gris bleuté), au-dessus de la brume (0,05 à 0,07). Tant que C4 n'a pas assombri les corps et
  hiérarchisé les fenêtres, la brume ne peut pas inverser l'ordre des valeurs. La séparation est tenue sur la formule (test
  `les_plans_sont_separes_en_valeur`, corps éteints à 20, 150 et 900 m, silhouette d'horizon sous la brume).

**Non fait / non tenu**
- Coût avant/après aux trois paliers : non mesuré (le rendu logiciel ne donne pas un temps GPU utilisable).
- Les panneaux de trafic (`traffic_panel.wesl`), les faisceaux et les hologrammes de graffiti n'appellent pas
  l'atmosphère : seuls façade, route, enseigne, véhicule et repères sont branchés (liste de la fiche).
- `KickWave` : sans `RoutingPlugin` (démos), les âges restent à 60 s ; la coquille n'a été vue que sur la scène forcée.
- Le vent de la brume ne déplace que les poussières (la formule analytique n'a pas de bruit 3D : arbitrage du plan).

**Tests rouges, tous antérieurs à C3** (mêmes que le journal de C2) : `cf-operator` `shot_names`, `cf-world-neon`
`signs::golden` (3) et `tests/sky` (2), `cf-audio` `player` (pas de carte son). `tests/signs` est vert. Clippy `-D warnings`
du workspace vert sous Rust 1.100.

**Environnement** : captures avec `apt-get install libxkbcommon-x11-0 mesa-vulkan-drivers` et
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`. **Disque** : `cargo test -p cf-world-neon` écrit ~14 Go de binaires
de test dans `target/debug/build/cf-world-neon/` ; les dossiers périmés s'y accumulent (256 variantes, 20 Go) et saturent la
session. Les supprimer entre deux passes (`find target/debug/build/cf-world-neon -maxdepth 1 -mmin +15 -exec rm -rf {} +`).

**Preuves** : 27 tests (`atmosphere::`), dont `proptest` de la transmittance contre intégration numérique, tube contre
intégration numérique, bornes de la phase et de la coquille, limiteur de la bouffée, indépendance à la cadence ; captures
`docs/captures/C3/` (avant, après, calme, sommet, coquille de kick, pluie).

**Prochaine session** : C4 · Sol, façades et matériaux (commencer par assombrir les corps de façade, voir les mesures).

### 2026-11-10 · C2 Image finale · ✅ (suite)

**Fait**
- Comparaison de tonemapping, la seule pièce manquante : `CF_TONEMAP=agx` dans `postfx_demo`, mêmes étapes, même scène,
  capture `docs/captures/C2/9-tonemap-khronos-contre-agx.png` (Khronos à gauche, AgX à droite).
- Mesures (rendu logiciel, étape « kick ») : Ȳ 0,141 (Khronos) contre 0,249 (AgX) ; saturation moyenne 0,173 contre 0,105 ;
  p99 de luminance 0,89 contre 0,79.

**Décision : `KhronosPbrNeutral` reste.** AgX relève les noirs (Ȳ ×1,8), désature d'environ 40 % et donne une image
voilée, à l'inverse du but de C2 (« tenir ses blancs et ses noirs ») et de la charte néon saturée. Khronos garde les noirs
profonds et les teintes de la palette. Pas de changement de code de rendu.

**Environnement** : la session n'avait ni `libxkbcommon-x11` ni pilote Vulkan logiciel (lavapipe) ; installés avec
`apt-get install libxkbcommon-x11-0 mesa-vulkan-drivers` pour les captures. À ajouter au hook de session si on veut
que les captures marchent sans manipulation.

**Tests rouges confirmés antérieurs à C2** : `cf-world-neon` `signs::golden` (3), `tests/signs` (2) et `tests/sky` (2) échouent à
l'identique sur `361407d` (fin de C1), rejoués ici. Ce sont des écarts de données Sillage (catalogue, slogans) et de décompte
d'entités du ciel (20 au lieu de 18), pas du post-traitement. `cf-operator` `shot_names` (plan `drone-pov` sans nom français) n'a
pas été rejoué avant C2 mais C2 ne touche pas `cf-operator`. À traiter dans un chantier dédié, pas ici. Total : 2413 réussis, 8 en échec,
clippy `-D warnings` vert.

**Passé ✅** : les critères non tenus sont consignés ci-dessus (`[~]`), aucun n'est caché. Restent à traiter ailleurs : Ȳ sur la vraie ville (C3/C4 en dépendent), référence de parité Sillage à régénérer,
test GPU de la remise à zéro de l'historique à la coupe, rayons à valider à l'œil.

### 2026-11-10 · C2 Image finale · 🟨

**Fait**
- Bloom : seuil 1,0, genou 0,5, queue `0,5 + 0,3·brume` (`PostFxSettings::mist`), pulsation inchangée.
- Étalonnage (`grade.wgsl`) : aberration ciblée (nulle pour r < 0,35, bornée à 0,01, enveloppe `TR` 2/90 ms), halation, rayons
  en espace écran (palier 0 seul), défilement UV glitché **[L]** (`ShowFx::glitch`, armé seulement si le limiteur accepte le flash).
- Grain : nouvelle passe `cf_grain` après le tonemapping, `lowbias32` (aucun `sin`), multiplicatif, 24 i/s, 0,06 → 0,03. `node.rs`
  est devenu générique (`FullscreenPass`) pour servir l'étalonnage et le grain.
- Persistance : `persist.wgsl` + `persist_node.rs`, deux textures demi-résolution créées à l'ouverture (recréées au redimensionnement),
  avant le bloom ; vidée à chaque coupe via `ShotCuts` (incrémenté par `cf-cinema`) et quand le palier de qualité la coupe.
- Outil de mesure `postfx::measure` (Ȳ, part émissive dans la palette, ice, histogramme de teintes, part du bloom), branché dans `postfx_demo`.
- Captures : `docs/captures/C2/` (8 étapes).

**Mesures (scène démo, rendu logiciel)** : Ȳ = 0,039, bloom = 20 % de Ȳ (≤ 25 %), ice 0,76 % (< 2 %), émissifs dans la palette 20 %
(la démo a une charte RVB et une ligne ambrée : critère 70 % pour C4, pas jugé ici).

**Décisions / écarts**
- Écart assumé à Sillage (règle 7) : seuil, genou, grain. La référence three.js (`tools/sillage-golden/postfx`) garde le grain additif ;
  les écarts de parité attendus ont grossi, non re-mesurés.
- Halation : 8 prises sur anneau dans le shader d'étalonnage, pas la 2ᵉ étage de la pyramide du bloom (inaccessible dans Bevy).
- Fraîcheur `E_f` de la persistance : approchée par `heat` (pas de fraîcheur dans `ShowFxInput`).
- Paliers : rayons au palier 0, halation et persistance jusqu'au 1.
- La toolchain de la session était en 1.97 ; mise à jour en 1.99 (exigence du dépôt).

**Non fait / non tenu**
- Comparaison `KhronosPbrNeutral` / `AgX` : non faite, KhronosPbrNeutral reste.
- Critère « Ȳ sur la scène de référence » : tenu sur la démo seulement ; Ȳ cible 0,06 non testé sur la ville.
- Tests de parité Sillage : pas de test automatique cassé, mais pas de nouvelle référence régénérée (bun/Playwright absents).
- Pas de test d'historique vidé à la coupe côté GPU (seulement la logique `Persistence`, testée).

**Tests rouges hors périmètre** (aucun fichier touché par C2) : `cf-operator` shot_names, `cf-world-neon` signs::golden (3), tests `signs` (2) et `sky` (2), et `cf-audio` `player` (`reordering_the_list…`, probablement sans carte son). Non confirmés sur un `main` propre. `cf-render` et `cf-cinema` sont verts, clippy du workspace aussi.

**Reprise** : AgX en capture côte à côte, régénérer la référence Sillage, vérifier la persistance avec un vrai défilement, puis passer ✅.

### 2026-11-10 · C1 Signaux et dramaturgie · ✅

**Fait**
- `cf-analysis::filters` : attaque/relâchement, crête avec maintien, One-Euro, borne de pente, ressorts critique et
  sous-amorti (intégration exacte), Schmitt, zone morte, z-score adaptatif, niveau dB + AGC + courbe.
- `cf-analysis::subbands` : `SB`/`PB` en biquads (cascade de deux, double précision, RMS sur 1024 échantillons),
  `LM`/`HM`/`TR` lus dans le spectre en octets déjà calculé pour le détecteur de kicks (aucune FFT de plus), flux
  z de `PB` et `TR`, HFC z, transitoire, spectre 32 bandes One-Euro. Champs **ajoutés** à `AudioFeatures`.
- `cf-render::audio::routing` : types `Route`, matrice RON validée (`default_routing.ron`, 22 routes qui reprennent
  P2 §2.2 et §2.3), chaîne de filtrage par route, `RoutingEngine`, `FilteredSignals`, `KickWave` (âges des 4
  derniers kicks), `RoutingPlugin` ajouté par `CfAudioPlugin`.
- `cf-show::density` : `Δ` et ses cinq états, fatigue, respiration imposée, pré-drop **[L]** ; publiée dans
  `ShowState::density` et `Res<VisualDensity>`.

**Décisions**
- One-Euro : lissages en `1 − exp(−2π·f·Δt)` au lieu de `1/(1+1/(2π·f·Δt))` (dépendant de la cadence) ; mesuré à
  moins de 1 % d'écart entre 30, 60 et 144 i/s.
- Les niveaux de sous-bande sont des RMS linéaires non normalisés ; AGC, courbe et seuil sont par route. Sur une
  capture, `SB`/`PB` voient le signal avant le gain automatique du micro, `LM`/`HM`/`TR` après : l'AGC par route
  absorbe l'écart.
- Onde de kick en **âges** (s) et non en horodatages : un `f32` perd la milliseconde après une heure d'horloge.
- Plusieurs routes sur une cible : maximum. Routes sous limiteur : un seul `FlashLimiter` (cadence, écart, plafond),
  chaque route a sa propre pente bornée à `1/MIN_FADE` ; flashs coupés ou respiration : sortie à 0.
- Pré-drop : la frappe n'est pas prévisible en direct. Le chef offre un creux seulement quand une montée a armé un
  drop, que l'intensité dépasse 0,7 et que la fin de phrase est à portée (jusqu'à 4 temps si le contraste du
  climax est insuffisant) ; un seul creux par frappe, par un `FlashLimiter` voie `Sweep` dédié à `Δ`.
- Fatigue de `Δ` comptée sur la densité voulue (avant plafond), sinon le plafond remettait le compte à zéro.
- Table de `Δ` : calme 0,35 / groove 0,65 / sommet 1,0 (DA donne des plages par état, pas la table).

**Écarts / non tenu**
- Pas de test d'allocation propre à `RoutingEngine::update` (aucun `Vec` dans la boucle par construction). La boucle
  d'analyse est couverte par `tests/no_alloc.rs` existant, qui passe avec les sous-bandes.
- `KickWave` et `FilteredSignals` ne sont pas encore des uniformes GPU : aucun shader ne les lit avant C2 à C4.
- `real_drop` n'est toujours pas détecté par l'analyse ; le pré-drop repose sur la montée détectée.
- Pas de capture : C1 ne change aucun rendu.
- Tests rouges **hors périmètre**, déjà présents sur `main` d'après leur objet et qu'aucun fichier de ce chantier ne
  touche : `cf-operator` `shot_names` (plan `drone-pov` sans nom français), `cf-world-neon` `signs::golden` (3 tests
  de catalogue de Sillage), `cf-world-neon` tests `signs` et `sky`. Non confirmé sur un `main` propre (recompilation
  trop longue) : à vérifier d'abord par la session suivante.

**Preuves** : 12 tests de filtres, 6 de sous-bandes (sinus 40/100 Hz > 6 dB, bruit rose, transitoires), 8 de
densité (+ instantané `insta`), 11 de routage ; clippy `-D warnings` du workspace vert.

**Prochaine session** : C2 · Image finale.
