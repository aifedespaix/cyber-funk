# cyber-funk : proposition d'architecture (étape 1)

> Statut : **proposition, en attente de validation.** Aucun code n'est écrit tant que ce document n'est pas validé.
>
> Source analysée : Sillage, `aifedespaix/aifedespaix.com` → `apps/main/src/components/sillage/` (≈ 37 400 lignes
> TypeScript + Svelte, Three.js / WebGL, ~150 fichiers moteur) et ses assets dans `apps/main/public/` (`music/`,
> `models/`, `sillage/`).

---

## 0. Ce que Sillage est vraiment (état des lieux)

Avant de proposer quoi que ce soit, voici ce que j'ai retenu du code, parce que la refonte en découle.

### Ce qui est solide et doit survivre au portage

| Brique Sillage | Fichiers | Pourquoi la garder |
| --- | --- | --- |
| **État du spectacle en lecture seule** : un objet unique `ShowState` publié à chaque image, lu par tous les systèmes, jamais écrit par eux | `show/types.ts`, `show/director.ts`, `show/state.ts` | C'est exactement le modèle d'une `Resource` ECS. Le portage est quasi 1:1. |
| **Portes (gates)** : chaque niveau dit ce qu'il *autorise* (plans, cascades, événements, taux de météores, flashs…), avec des nombres qui glissent sans saut d'un niveau à l'autre | `show/acts.ts`, `show/gates.ts` | Excellente idée : on ne scénarise pas l'image, on borne l'improvisation. À garder, mais en **données** et non en code. |
| **Chef d'orchestre du drop** : le drop est *préparé* pendant la montée (plan caméra, cascade, paquet d'effets) puis tout frappe sur la même image | `show/drop.ts`, `show/build.ts`, `cinema/choreo.ts` | C'est ce qui donne l'effet « l'image sait ce qui arrive ». Indispensable en VJing. |
| **Fraîcheur (anti-répétition)** commune à toutes les familles (`shot:`, `stunt:`, `event:`…) | `show/freshness.ts` | Simple, efficace, se porte tel quel. |
| **Réalisateur caméra** : catalogue de 28 plans (`ShotSpec` : rang, plage d'énergie, durée, cooldown, opportunité), transitions cut/blend/swoop/whip calées sur les temps | `cinema/*` | Le cœur du « mode Cinéma ». Le contrat `Shot`/`ShotSpec` est bon, on le traduit en trait Rust. |
| **Analyse audio maison** : flux d'énergie par bande, kicks, BPM par médiane des intervalles, sections calme/groove/sommet avec hystérésis, gain automatique du micro par bande | `beat.ts`, `sections.ts`, `audio.ts` | Testé en conditions réelles, réglé finement. On le porte d'abord tel quel (avec tests de non-régression), on l'améliore ensuite. |
| **Limiteur photosensible** WCAG 2.3.1 (≤ 3 flashs/s, pas de rouge saturé, fondus minimaux) | `show-fx-limiter.ts` | Obligatoire pour un public. Non négociable. |
| **Qualité adaptative** (résolution de rendu, bloom, cadence des rendus annexes) avec hystérésis et patience exponentielle | `quality.ts` | Le principe reste ; l'implémentation change (voir §1.4). |

### Ce qui pose problème (et que la refonte doit corriger)

1. **Le jeu et le spectacle sont emmêlés.** `sillage-scene.ts` (982 lignes) orchestre à la main, dans un ordre figé,
   ~40 systèmes dont la moitié n'a de sens qu'en mode Jeu (volant, boîte de vitesses, score, bonus, arches, cockpit
   interactif, commandes de vol). Le mode Cinéma est un « mode » du jeu, alors que c'est le produit.
2. **Toute la dramaturgie est en dur dans le code.** Les six niveaux (`ACTS`) et leurs moments de bascule
   (`LEVEL_STARTS = [0, 0, 0.06, 0.18, 0.35, 0.55]` du set) sont des constantes TypeScript. Ajouter un « acte » ou en
   modifier un demande un développeur et un redéploiement.
3. **Un seul set linéaire pour toute la soirée.** L'horloge du set fait monter le niveau avec le temps total. Avec
   plusieurs DJs, le DJ de 23 h hérite du niveau 3 et celui de 2 h du niveau 5, qu'ils jouent de la deep house ou du
   hardcore. Un DJ n'est qu'un **nom + un thème de couleurs** (`ShowDj`), rien de plus.
4. **Contrôles opérateur pauvres et fragiles en live.** Une vingtaine de lettres du clavier (`operator.ts`), pas de MIDI,
   pas de vue de ce qui va arriver (« next cue »), pas de verrouillage contre une fausse manipulation.
5. **Le monde est défini par rapport à la voiture.** Le repère, la caméra et le streaming de la ville sont pensés
   « une voiture file vers −z ». C'est très bien pour ce monde-là, mais ça empêche d'ajouter un autre univers.
6. **Culling quasi absent.** `frustumCulled = false` sur la plupart des gros meshes (brume, décor, traînée, pneus, ciel,
   faisceaux) : WebGL dessine tout, tout le temps. La mémoire est déjà bornée par le recyclage de la route, mais le GPU
   paye chaque objet hors champ.
7. **Persistance minimale.** Le `localStorage` garde `elapsed`, `level`, `djIndex` pendant 15 min. Pas de fichier de
   show portable, pas de préparation à la maison, pas de reprise après plantage fiable.
8. **Musiques intégrées : attention aux licences.** Seules 4 pistes sur 26 sont listées CC0 dans `music/LICENCES.md`.
   Les autres (Darude, The Prodigy, Hotline Miami…) ne peuvent pas être embarquées dans un binaire distribué.

### Ce qu'on supprime (mode Jeu et spécificités web)

`controls.ts`, `cockpit-controls.ts`, `gearbox.ts`, `shift-gate.ts`, `score.ts`, `pickups.ts`, `arches.ts`,
`flight/input.ts`, `race-line.ts`, `sfx.ts` (bruitages voiture : la musique est celle du DJ), `car-preview.ts`, tous les
composants Svelte de jeu (`SillageSpeedo`, `SillageCarPicker`, `SillageFlightPad`, `SillageHud`, `SillagePause`…), le SEO,
la PWA, le `localStorage`. La **voiture reste** comme sujet filmé en pilote automatique (`autopilot.ts`, `dance.ts`,
`flight/pilot.ts`, `stunts/*`), mais plus personne ne la conduit.

La caméra libre (`free-camera.ts`) est conservée **uniquement** dans l'outil de préparation, pour cadrer et déboguer.

---

## 1. Choix du moteur 3D

### 1.1 Les candidats

| Option | Pour | Contre | Verdict |
| --- | --- | --- | --- |
| **Bevy** (ECS + renderer sur wgpu) | ECS parallèle ; frustum culling automatique par entité ; batching/instancing automatique ; préprocessing et culling GPU ; occlusion culling (deux passes) ; HDR, bloom, tonemapping, post-process via render graph ; chargeur glTF ; multi-fenêtres natif ; rechargement à chaud des assets ; écosystème (egui, diagnostics). Tout reste ouvert : on peut descendre à wgpu dans un nœud de rendu custom. | Casse son API à chaque version (~tous les 3-4 mois). Temps de compilation. Son module audio est trop limité pour notre usage (on ne s'en servira pas). | **Recommandé** |
| **wgpu pur** | Contrôle total, binaire minimal, zéro surcoût d'abstraction | Il faut tout réécrire : graphe de scène, glTF, PBR, bloom, texte, fenêtrage, assets, culling. Sillage contient des dizaines de matériaux et de systèmes : des mois de travail avant le premier visuel. | Rejeté (mais disponible *sous* Bevy) |
| **Fyrox** | Éditeur complet, moteur mature | Éditeur orienté jeu, renderer moins moderne, communauté plus petite, pas d'ECS | Rejeté |
| **three-d / rend3 / Kajiya** | Légers ou très avancés | rend3 n'est plus maintenu, Kajiya est expérimental, three-d est trop limité pour ce volume | Rejetés |
| **Godot (gdext Rust)** | Éditeur, rendu riche | Le Rust n'est qu'un langage de script ; le pipeline audio/MIDI basse latence serait à contourner ; on ne contrôle pas la boucle | Rejeté |

### 1.2 Pourquoi Bevy colle à ce projet précis

- **L'architecture de Sillage est déjà un ECS qui s'ignore.** Des systèmes indépendants lisent un état partagé en
  lecture seule (`ShowState`, `BeatState`, `CinemaWorld`) et ne se parlent pas. En Bevy : `ShowState`, `AudioFeatures`
  et `CinemaWorld` deviennent des `Resource`, chaque `*.update()` devient un système, et l'ordre écrit à la main dans
  `sillage-scene.ts` devient des `SystemSet` ordonnés et parallélisés par le scheduler.
- **Culling fourni et extensible** (détails en §1.4) : frustum culling par `Aabb`, `VisibilityRange` pour les LOD,
  occlusion culling pour la ville dense, `RenderLayers` pour les plans avant/arrière.
- **Multi-fenêtres natif** : une fenêtre projecteur plein écran sans aucune interface + une fenêtre opérateur.
- **Les shaders GLSL de Sillage se portent en WGSL** dans des `Material` / `ExtendedMaterial` : même logique, syntaxe
  différente, et rechargement à chaud pendant le développement.

**Version :** la dernière version stable de Bevy au moment du jalon J0 (vérifiée à ce moment-là, changelog lu). On la
fige au niveau mineur, et chaque montée de version fait l'objet d'une PR dédiée.

**Gestion du risque « API instable » :** toute la logique métier (audio, analyse, show, contrôle) vit dans des crates
**sans dépendance à Bevy** (voir §1.5). Une montée de Bevy ne touche que la couche de rendu et la colle ECS.

### 1.3 Audio et MIDI : hors de Bevy, sur leurs propres threads

Le rendu tourne à 60 Hz ; l'audio et le MIDI ne doivent jamais attendre une image.

| Besoin | Crate | Rôle |
| --- | --- | --- |
| Entrée micro / carte son | `cpal` | Capture bas niveau, choix du périphérique (interface USB du DJ, retour booth, micro). |
| Lecture des pistes de démo | `symphonia` (décodage) + `cpal` (sortie), ou `kira` | Lecture avec horloge d'échantillons précise, utile pour l'analyse hors ligne. |
| FFT | `realfft` (`rustfft`) | Spectre à pas fixe sur le thread d'analyse. |
| Transport sans verrou | `rtrb` (ring buffer) + `triple_buffer` | Callback audio → thread d'analyse → monde ECS, sans mutex ni allocation. |
| MIDI | `midir` | Entrées/sorties MIDI sur Windows, macOS, Linux. Notes/CC des contrôleurs, **MIDI Clock** (24 ppqn), Start/Stop, Song Position. |
| Synchro tempo réseau | `rusty_link` (Ableton Link), optionnel | Tempo et phase exacts depuis Traktor, Rekordbox (mode Link), Ableton, etc. |
| OSC | `rosc`, optionnel | Pilotage depuis TouchOSC, Resolume, scripts. |
| CDJ Pioneer (Pro DJ Link) | à évaluer, plus tard | Beat, mesure et *phrases* analysées par rekordbox, directement depuis les platines. Puissant mais protocole non officiel : jalon optionnel. |

**Pipeline audio :**

```
cpal callback (thread temps réel, ~2-5 ms)
   └─▶ rtrb ring buffer (échantillons bruts, aucune allocation)
         └─▶ thread d'analyse (pas fixe : 512 échantillons @ 48 kHz ≈ 10,7 ms)
               FFT → bandes → flux → kicks horodatés → BPM → sections
               └─▶ triple buffer : AudioFeatures (dernier état)
               └─▶ canal : événements datés (Kick, Bar, Phrase, Drop…)
                     └─▶ système Bevy (début d'image) : Resource<AudioFeatures> + Events
```

Gain par rapport à Sillage : l'analyse Web Audio tourne une fois par image (~16 ms, sur un `AnalyserNode` lissé).
Ici elle tourne à pas fixe (~10 ms) et chaque kick est **horodaté**, ce qui permet de compenser la latence du
vidéoprojecteur (réglage « décalage visuel » en ms).

**Arbitrage des sources de tempo** (affiché à l'opérateur avec un indicateur de confiance) :

1. MIDI Clock ou Ableton Link, si présents : tempo et phase exacts.
2. Détection de kicks sur l'entrée audio (micro ou ligne) : le portage de `beat.ts`.
3. Tap tempo (touche ou pad), en secours.

Les **réactions à l'énergie** (basses, médiums, aigus, intensité, sections) viennent toujours de l'audio, même quand le
tempo vient du MIDI.

### 1.4 Performances GPU et culling

Précision importante sur la demande : le **frustum culling économise du travail GPU et CPU** (on ne dessine pas, on
n'anime pas ce qui est hors champ), mais **pas de mémoire**. La mémoire, elle, se maîtrise par le **streaming et le
recyclage** du monde. Il faut les deux.

| Technique | Ce qu'elle évite | Mise en œuvre |
| --- | --- | --- |
| **Frustum culling** | Dessiner ce qui est hors champ | Natif Bevy (par entité, `Aabb`). On interdit `NoFrustumCulling` sauf justification écrite ; les meshes « suiveurs de caméra » de Sillage (brume, pluie, traînée) reçoivent un `Aabb` correct au lieu d'être exclus du culling. |
| **Occlusion culling** | Dessiner les immeubles cachés derrière d'autres | Occlusion culling GPU de Bevy (deux passes, depth prepass). Gros gain dans une rue bordée d'immeubles. À valider sur le matériel cible : si la fonction est encore expérimentale, on l'active derrière un réglage. |
| **Culling de simulation** | Animer ce qui n'est pas vu | Les systèmes coûteux (trafic, enseignes animées, landmarks) filtrent sur `ViewVisibility` ou sur une distance de simulation ; hors champ, mise à jour à cadence réduite ou gel. |
| **Streaming par tronçons + pools** | Saturer la mémoire, allocations en live | Le monde est découpé en tronçons le long de la route ; ceux qui sortent derrière la caméra sont **recyclés** (pool d'entités), jamais détruits/recréés. Zéro allocation en régime établi. |
| **LOD / visibility ranges** | Géométrie fine au loin | `VisibilityRange` (fondu entre niveaux de détail) ; au-delà de la brume, imposteurs ou rien. |
| **Instancing / batching** | Un draw call par objet | Batching automatique de Bevy (même mesh + même matériau) ; atlas de textures pour les façades et enseignes. |
| **Rendus annexes cadencés** | Rendre la scène 3 fois par image | Le rétroviseur disparaît (c'était du jeu). Les enseignes « caméra live » de Sillage sont rendues à basse résolution et à cadence réduite, et seulement si elles sont visibles. |
| **Qualité adaptative** | Les saccades en live | Portage de `quality.ts` : échelle de rendu dynamique, puis bloom, puis densité de particules. Budget mesuré au GPU (requêtes de timestamp wgpu), pas seulement au temps d'image CPU. |

**Budget cible (à confirmer selon ton matériel) :** 1080p à 60 i/s stables sur une carte milieu de gamme (classe
RTX 3060 / Apple M1 Pro), 4K à 30 i/s en option. Une surcouche de diagnostic (touche dédiée, jamais sur la sortie
projecteur) affiche : i/s, ms GPU par passe, draw calls, entités visibles / cullées, tronçons actifs, mémoire GPU.

### 1.5 Plans avant et arrière : rendu en couches

La demande « afficher des éléments devant la caméra et en arrière-plan » se traite par un **empilement de caméras**
qui partagent la même pose mais pas les mêmes objets (`RenderLayers` + `Camera::order`) :

```
 ordre  couche            contenu                                              profondeur
 ─────  ────────────────  ───────────────────────────────────────────────────  ────────────────────────────
   0    ARRIÈRE-PLAN      ciel, étoiles, planètes, géante à anneau, nébuleuses   à part (infini, parallaxe via skyDrift)
   1    MONDE             ville, route, voiture, trafic, enseignes, météores     tampon de profondeur normal
   2    AVANT-PLAN 3D     objets en repère caméra : logo du DJ en hologramme,    profondeur effacée → jamais coupé
                          particules devant l'objectif, pluie sur l'objectif,     par un mur du monde
                          texte 3D, « lens dirt »
   3    SURCOUCHE 2D      cartons-titres, nom du DJ, bandeau, masques            aucune
   ──   POST-PROCESS      bloom, grade, aberration, flashs (via le limiteur)    sur l'image composée
```

Chaque couche est activable et paramétrable **par look** (§2). Exemple : un look « Cosmos » renforce l'arrière-plan
et coupe le monde ; un look « Hommage DJ » met son logo en avant-plan pendant que la ville continue derrière.

C'est la généralisation de ce que Sillage faisait déjà en bricolage pour la vue cockpit (passe rendue après le monde,
profondeur effacée, dans `postfx.ts`).

### 1.6 Découpage en crates

```
cyber-funk/
├── Cargo.toml                    workspace
├── crates/
│   ├── cf-audio/                 capture (cpal), lecture des démos (symphonia), ring buffers      ─┐
│   ├── cf-analysis/              FFT, kicks, BPM, sections, AudioFeatures (portage beat/sections)  │ sans Bevy :
│   ├── cf-control/               MIDI (midir), clock, Link, OSC, clavier abstrait, Actions, learn  │ testables seuls,
│   ├── cf-show/                  modèle Événement/DJ/Cue/Look, chef du spectacle, portes, drop,    │ insensibles aux
│   │                             fraîcheur, limiteur de flashs, sauvegarde (serde + RON)          ─┘ montées de Bevy
│   ├── cf-render/                plugin Bevy : couches, post-process, qualité, diagnostics
│   ├── cf-cinema/                plugin Bevy : réalisateur, catalogue de plans, transitions
│   ├── cf-world-neon/            plugin Bevy : le monde de Sillage (ville, route, ciel, voiture, trafic…)
│   ├── cf-operator/              fenêtre opérateur (bevy_egui) : live + préparation
│   └── cyber-funk/               binaire : assemble les plugins, lit la config
├── assets/
│   ├── models/  shaders/  textures/
│   ├── music/                    pistes de démo (CC0 uniquement dans le dépôt)
│   ├── looks/*.ron               looks livrés avec l'app
│   └── shows/demo.ron            un show d'exemple
└── docs/
```

Le monde « Neon City » est **un plugin parmi d'autres** qui implémente un trait `World` (ce qu'il peut filmer,
comment il réagit aux portes). Un deuxième univers pourra s'ajouter sans toucher au reste.

---

## 2. Refonte UX : du « set linéaire » au « show en cues »

### 2.1 Le constat

Sillage fonctionne comme un **pilote automatique** : on règle une durée, le niveau monte avec le temps, la machine
improvise. C'est formidable pour un set unique d'un DJ unique qu'on ne veut pas surveiller. Ça ne tient plus quand :

- plusieurs DJs se succèdent avec des styles et des durées différents ;
- un opérateur veut **décider** du moment d'un changement (le DJ annonce son dernier morceau, un invité monte, un drop
  attendu) ;
- on veut **préparer** la soirée à l'avance et la rejouer à l'identique.

Le monde du spectacle vivant a résolu ce problème depuis longtemps avec la **liste de cues** (conduite lumière, théâtre,
QLab, consoles grandMA). Je propose d'adopter ce modèle, **sans perdre l'improvisation de Sillage** : un cue ne
scénarise pas l'image, il **règle les portes** à l'intérieur desquelles le réalisateur improvise.

### 2.2 Le nouveau modèle de données

```
Événement (fichier .cfshow, RON)
├── réglages globaux : nom, politique de flashs, sorties vidéo, décalage de latence, mapping MIDI commun
├── bibliothèque de Looks (en plus de ceux livrés avec l'app)
└── Créneaux DJ (ordre de passage)
    └── Créneau
        ├── identité : nom, logo (image), thème de couleurs, horaires prévus
        ├── profil audio : plage de BPM attendue, sensibilité, source de tempo préférée
        ├── profil de contrôle : mapping MIDI propre (son contrôleur, sa table de mix)
        └── Liste de cues
            └── Cue
                ├── look : référence à un Look (+ surcharges éventuelles)
                ├── entrée : coupe / fondu N mesures / « au prochain drop » / « à la prochaine phrase »
                ├── déclencheurs ponctuels à l'entrée : carton-titre, logo du DJ, Ascension, événement…
                └── avancée : manuelle (GO) / après N mesures / au prochain drop / à une heure / après N minutes
```

**Look** = ce qu'était un « acte » de Sillage, mais en données, éditable et rechargeable à chaud :

```ron
// assets/looks/ville-qui-monte.ron  (exemple illustratif)
Look(
    name: "La ville qui monte",
    world: "neon",
    layers: (background: 1.0, world: 1.0, foreground: 0.3),
    palette: Inherit,                       // ou Preset("synthwave"), ou celle du DJ
    gates: (
        max_floor: 4, tricks: true,
        stunts: ["off-road", "slalom", "threading", "wall-ride"],
        shots: (tiers: [Common, Uncommon, Rare], block: ["ringworld"]),
        cut_grid: (calm: 8, groove: 4, peak: 4),
        events: ["chase", "convoy", "tower", "race"],
        meteor_rate: 1.5, cruise: 1.07,
    ),
    fx: (rgb_split: 0.3, flash: 0.3, city_pulse: 0.6),
    macros: [                               // jusqu'à 8 boutons rotatifs assignables
        Macro(name: "Chaos",   drives: [("gates.event_rate", 0.5, 2.0), ("gates.stunt_gap", 1.5, 0.6)]),
        Macro(name: "Vitesse", drives: [("gates.cruise", 0.9, 1.2)]),
        Macro(name: "Couleur", drives: [("palette.hue_shift", -60.0, 60.0)]),
    ],
)
```

Les six niveaux actuels de Sillage deviennent **six looks livrés avec l'app** (`niveau-0-pre-intro` …
`niveau-5-cosmos`), et le comportement actuel devient **un modèle de liste de cues** (« Montée automatique ») : rien
n'est perdu, et un DJ qui ne veut rien préparer garde l'expérience Sillage.

### 2.3 Trois façons de piloter, à choisir par créneau

| Mode | Qui décide des changements de cue | Pour qui |
| --- | --- | --- |
| **Auto** | Les règles d'avancée (temps, drops, mesures) : le comportement Sillage | Pas d'opérateur, ou DJ seul |
| **Semi-auto** (défaut) | L'opérateur appuie sur GO ; si personne n'appuie, les règles d'avancée prennent le relais | Opérateur qui surveille de loin |
| **Manuel** | Uniquement l'opérateur (ou les notes MIDI du DJ) | VJ dédié, set très écrit |

Dans les trois modes, le réalisateur **continue d'improviser** dans les portes du cue en cours : choix des plans, des
événements, préparation des drops. L'opérateur pilote la dramaturgie, pas chaque image.

### 2.4 Une seule couche d'Actions pour tous les contrôles

Tout ce qui entre (clavier, notes et CC MIDI, MIDI Clock, OSC, Link) est traduit en **Actions** par une table de
correspondance. Le moteur ne connaît que les Actions.

| Famille | Actions |
| --- | --- |
| Conduite | `Go`, `Back`, `GotoCue(n)`, `Hold` (fige l'avancée auto), `NextDj`, `PrevDj`, `ArmDj(n)` |
| Ponctuels | `ForceDrop`, `SkipShot`, `ShowDj`, `Title(texte)`, `Trigger(événement)`, `Ascension` |
| Continus | `Macro(i, 0..1)`, `Master(0..1)` (intensité globale), `LayerFade(couche, 0..1)` |
| Sécurité | `Blackout`, `SafeLook` (repli sur un look calme), `FlashKill`, `Breathing` |
| Tempo | `TapTempo`, `Resync` (recaler la mesure sur le prochain kick), `NudgePhase(±)` |

- **MIDI learn** : dans l'interface, on clique sur une action, on touche un pad ou un potard, c'est assigné.
- **Profils** : un mapping par contrôleur, attaché à l'événement ou au créneau d'un DJ (chaque DJ arrive avec son
  matériel).
- **Signaux du logiciel de mix** : Traktor, Rekordbox, Serato et VirtualDJ savent envoyer MIDI Clock et/ou des notes
  MIDI sur des boutons choisis (hot cue, effet, changement de deck). On peut donc associer « le DJ lance son effet
  écho » → `Trigger("storm")`, ou « hot cue 8 » → `Go`.
- **Le clavier reste disponible**, avec les raccourcis de Sillage comme valeurs par défaut (D, N, flèches, 0-5, G, T…),
  désormais reconfigurables.

### 2.5 Deux fenêtres, deux usages

**Sortie projecteur** : plein écran sur l'écran choisi, aucune interface, jamais. (Plus tard, en option : sortie
Spout/Syphon/NDI pour passer par une régie vidéo ou Resolume.)

**Fenêtre opérateur**, avec deux modes distincts :

- **Préparation** (à la maison) : éditeur d'événement, de créneaux, de cues et de looks ; aperçu du look en direct ;
  caméra libre pour vérifier un cadrage ; MIDI learn ; import/export du fichier `.cfshow`.
- **Live** (pendant la soirée) : édition verrouillée, gros éléments lisibles dans le noir :

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ ◀ DJ précédent   [ 22:00 Aya ▸ 23:30 KRMN (en cours, 47:12) ▸ 01:00 Lune ]   DJ suivant ▶ │
├───────────────────────────────────────────────┬──────────────────────────────────────────┤
│  CUES de KRMN                                  │  APERÇU de la sortie (basse résolution)  │
│   3  La ville qui monte          ✔ joué        │                                          │
│ ▶ 4  Démesure            (auto dans 6 mesures) │                                          │
│   5  Cosmos              entrée : au drop      │                                          │
│   6  Hommage KRMN        logo avant-plan       ├──────────────────────────────────────────┤
│                                                │ TEMPO 128,0  MIDI Clock ●  confiance ███ │
│        [   GO   ]   [ BACK ]   [ HOLD ]        │ basses ▆▆▆▅  méd ▃▃▄  aigus ▂▂▃          │
│                                                │ section : groove → montée détectée       │
├───────────────────────────────────────────────┤ prochain drop préparé : wheel + slalom   │
│ MACROS  Chaos ◐  Vitesse ◑  Couleur ◔  Master ●│ 60 i/s · GPU 9,8 ms · 1 412 objets vus   │
│ PADS  [DROP] [TITRE] [LOGO DJ] [ÉVÉNEMENT]     │ MIDI : X1 mk3 ●  DDJ-FLX4 ●              │
│ SÉCURITÉ  [BLACKOUT] [LOOK CALME] [FLASHS ✕]   │                                          │
└───────────────────────────────────────────────┴──────────────────────────────────────────┘
```

### 2.6 Passage d'un DJ à l'autre

Sillage a déjà un fondu de thème calé sur un temps fort (`show/dj-transition.ts`) et une enseigne géante du DJ.
Je propose d'en faire une **séquence de passation** : on **arme** le DJ suivant (`ArmDj`), puis `NextDj` joue sa
transition (fondu de palette sur une mesure, logo en avant-plan, enseigne) et charge son créneau : son profil audio,
son mapping MIDI et le premier cue de sa liste. Un créneau peut aussi reprendre là où la soirée en est (« continuer
l'intensité ») au lieu de repartir de zéro.

### 2.7 Fiabilité en live

- **Sauvegarde continue** de l'état (créneau, cue, temps écoulé, macros) dans un fichier, toutes les quelques
  secondes, écriture atomique. Au redémarrage après un plantage : reprise proposée en un clic.
- **Look de secours** toujours prêt, accessible par une action (`SafeLook`) et utilisé automatiquement si un look ne
  se charge pas.
- **Aucun chargement d'asset pendant le show** : tout ce que la liste de cues utilise est préchargé et les shaders
  sont compilés à l'ouverture de l'événement (pas d'accroc de compilation en plein drop).
- **Perte de source** : si le MIDI Clock disparaît, bascule sur la détection audio sans coupure ; si l'audio
  disparaît, le monde ralentit doucement comme dans Sillage (`live = false`).
- **Limiteur de flashs** appliqué en dernier, quelles que soient les demandes des looks ou de l'opérateur.

---

## 3. Plan d'action

Chaque jalon se termine par une démo exécutable, `cargo fmt`, `cargo clippy -D warnings` et `cargo test` au vert.
Pas de passage au jalon suivant avec un avertissement ouvert.

| Jalon | Contenu | Démo de fin de jalon |
| --- | --- | --- |
| **J0 Fondations** | Workspace Cargo, découpage en crates (§1.6), CI GitHub Actions (fmt, clippy, tests, build Windows/macOS/Linux), `tracing` pour les journaux, version de Bevy figée, compilation rapide en dev (`dynamic_linking`, linker rapide). Deux fenêtres : projecteur et opérateur. | Deux fenêtres, un cube qui tourne, CI verte. |
| **J1 Audio** | `cf-audio` + `cf-analysis` : lecture des démos, capture micro/ligne, thread d'analyse, portage de `beat.ts`, `sections.ts` et du gain automatique. **Tests de non-régression** : j'exporte depuis Sillage les kicks, BPM et sections détectés sur 3-4 pistes et je compare à la sortie Rust. | Vumètres, BPM et sections en direct dans la fenêtre opérateur, au micro et sur les démos. |
| **J2 Cœur du show** | `cf-show`, sans rendu : modèle Événement/Créneau/Cue/Look, fichiers RON, chef du spectacle (portage de `show/director.ts`, `gates.ts`, `drop.ts`, `freshness.ts`, `time-scale.ts`), limiteur de flashs, sauvegarde et reprise. Les 6 niveaux deviennent 6 looks. | Simulation sans image : un set entier rejoué en accéléré avec la trace des cues, drops et portes, sous tests. |
| **J3 Contrôles** | `cf-control` : couche d'Actions, clavier, MIDI (`midir`) avec learn et profils, MIDI Clock, puis Ableton Link et OSC. | Un contrôleur MIDI fait GO/BACK et tourne les macros ; le tempo suit le MIDI Clock de Traktor ou Rekordbox. |
| **J4 Socle de rendu** | `cf-render` : couches arrière/monde/avant/2D, HDR, bloom, grade (portage de `postfx-grade.ts`), flashs via le limiteur, streaming par tronçons avec pools, culling vérifié par la surcouche de diagnostic, qualité adaptative. | Une ville de boîtes génériques sur une route infinie à 60 i/s, avec compteurs de culling. |
| **J5 Le monde Neon City** | `cf-world-neon`, première passe : route, immeubles et matériaux de façade, ciel et planètes, brume, pluie, voiture (glTF) en pilote automatique, phares. Shaders GLSL → WGSL. | La ville de Sillage reconnaissable, réactive à la musique, sans caméra intelligente. |
| **J6 Le réalisateur** | `cf-cinema` : trait `ShotSpec`/`Shot`, transitions, repères musicaux, éditeur et chorégraphie du drop, puis portage des 28 plans par lots (communs d'abord). | Le mode Cinéma de Sillage, plans et coupes sur les temps. |
| **J7 Contenu** | Par ordre d'impact visuel : enseignes et enseigne du DJ, trafic et ses 23 événements, 12 cascades, landmarks et vitrines, météores, effets. C'est le plus gros volume, mené en lots indépendants. | Parité visuelle avec Sillage, lot par lot. |
| **J8 Interface opérateur** | `cf-operator` complet : préparation (éditeurs) et live (§2.5), passation des DJs, aperçu, MIDI learn, import/export `.cfshow`. | Préparer une soirée à 3 DJs et la jouer de bout en bout. |
| **J9 Prêt pour le live** | Test d'endurance de 6 h (fuites mémoire, dérive), coupures de sources simulées, préchargement complet, mesures sur le matériel cible, paquets d'installation par plateforme. | Une soirée blanche sans intervention. |

**Ordre de priorité si on doit raccourcir :** J0 → J1 → J2 → J3 → J4 → J5 → J6 donne déjà un outil de VJing
utilisable en soirée (ville + caméra intelligente + cues + MIDI). J7 et J8 enrichissent, J9 sécurise.

**Méthode de portage :** on traduit par module en gardant les noms et les contrats de Sillage (`ShowState`, `ShotSpec`,
`Gates`…) pour que la correspondance reste lisible, avec les commentaires en français comme dans le projet d'origine.
Les constantes réglées à la main (seuils, durées, poids) sont reprises à l'identique, puis exposées dans les looks
quand elles relèvent de la mise en scène.

---

## 4. Questions à trancher avant de coder

Mes réponses par défaut entre parenthèses ; je les applique si tu n'as pas d'avis.

1. **Matériel cible** : quel ordinateur et quelle carte graphique en soirée ? Quel OS ? (Windows + GPU NVIDIA milieu
   de gamme, macOS Apple Silicon en second.)
2. **Sortie vidéo** : un projecteur en 1080p, un mur LED à résolution exotique, plusieurs écrans ? (Un seul écran
   1080p ou 4K.)
3. **Matériel des DJs** : Rekordbox, Traktor, Serato, CDJ autonomes ? Cela fixe la priorité entre MIDI Clock, Ableton
   Link et Pro DJ Link. (MIDI Clock + Link d'abord, Pro DJ Link plus tard.)
4. **Qui pilote** : un VJ dédié à côté des DJs, ou les DJs eux-mêmes depuis leur contrôleur ? (Les deux, mode
   semi-auto par défaut.)
5. **D'autres univers que Neon City** à court terme, ou d'abord la parité avec Sillage ? (Parité d'abord, mais
   l'architecture accueille un deuxième monde dès J4.)
6. **Musiques intégrées** : on embarque seulement les 4 pistes CC0 dans le dépôt, et les autres restent dans un
   dossier local non versionné, chargé au démarrage ? (Oui.)
7. **Sortie Spout/Syphon/NDI** vers une régie vidéo : nécessaire ? (Non au départ, jalon optionnel après J9.)
