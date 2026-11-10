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

- [ ] Sur la scène de référence : `Ȳ` et part du bloom dans les bornes de DA §3.1.
- [ ] Grain : noirs inchangés (pixel à 0 reste à 0), aucune fonction `sin` dans le hachage.
- [ ] Aberration nulle dans le disque central de rayon 0,35.
- [ ] Persistance : aucun flash prolongé (test du limiteur), historique vidé à la coupe.
- [ ] Captures avant/après dans `docs/captures/C2/` ; tests de parité Sillage mis à jour (règle 7).

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

- [ ] Test de formule : `T` analytique contre intégration numérique (écart < 1 %), limite `u → 0` stable.
- [ ] Sur capture : luminance moyenne des plans P1, P2, P3 séparée d'au moins 0,12 ; P1 plus sombre que la brume.
- [ ] Aucune passe plein écran ajoutée ; coût mesuré avant/après aux trois paliers.
- [ ] Captures `docs/captures/C3/` : calme, sommet, coquille de kick.

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
- [ ] Une fenêtre ne dépasse jamais `0,9·θ_b` (test du shader ou de la fonction miroir en Rust).
- [ ] Toujours un seul lot de dessin pour les pavés de la ville.
- [ ] Captures `docs/captures/C4/` : sec, mouillé, kick, calme contre sommet.

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
| C2 | Image finale | 🟨 | voir `git log` (« C2 ») | Reste : comparaison AgX / KhronosPbrNeutral, rayons (à valider à l'œil), Ȳ sur la vraie ville. Matériel cible : cadence, bloom fin, couleurs sur projecteur. |
| C3 | Atmosphère et profondeur | ⬜ | — | — |
| C4 | Sol, façades et matériaux | ⬜ | — | — |
| C5 | Ville : distribution et archétypes | ⬜ | — | — |
| C6 | Ville : habillage | ⬜ | — | — |
| C7 | Tunnel, événements, LOD et culling | ⬜ | — | — |
| C8 | Personnages et tenues | ⬜ | — | — |
| C9 | Mise en scène et signatures | ⬜ | — | — |

## Journal

Une entrée par session, la plus récente en haut : date, chantier, ce qui est fait, décisions prises, critères
non tenus et pourquoi, point de reprise si le chantier reste 🟨.

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
