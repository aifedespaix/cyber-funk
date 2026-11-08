# Séquence de démarrage et d'arrêt du show 3D, caméra d'attente (projet 2)

Date : 2026-11-08. Suite du projet 1 (`2026-11-08-operateur-refonte-design.md`). Le projet 3 (changement de DJ : thème
et nom sur la voiture) est hors périmètre.

> Les durées, styles et trajets ci-dessous sont **mes choix** (l'utilisateur m'a laissé décider : « un truc de qualité et
> bien pensé »). Ils sont regroupés dans une seule table de constantes (§4) pour se régler sans toucher à la logique.

## 1. Intention

Aujourd'hui `Démarrer` / `Arrêter` ne changent que l'état affiché dans la fenêtre opérateur : le monde 3D, la musique et
le réalisateur tournent dès le lancement. On veut que l'état soit **réel** :

- **Waiting** : la caméra reste sur la route, mouvement d'attente lent et agréable. Route sombre, aucun immeuble, aucune
  voiture, aucune circulation, aucun événement. Rien ne joue.
- **Starting** : la musique démarre, la voiture se matérialise sur une route vide et roule, les immeubles apparaissent
  progressivement (façon Matrix), le ciel et les fenêtres s'allument, puis l'intro du DJ (enseigne + carte de titre).
  À la fin : `Running`.
- **Stopping** : les fenêtres s'éteignent par vagues, les immeubles se désynchronisent (glitch) et rétrécissent en
  s'enfonçant dans le sol, la voiture freine, s'arrête, s'éteint (effet « vieille télé qui s'éteint ») et disparaît, la
  musique s'estompe, la caméra revient à la scène d'attente. À la fin : `Waiting`.

Succès : l'opérateur appuie, voit une séquence propre et sans à-coup, retrouve exactement la scène d'attente, et peut
recommencer indéfiniment (soak) sans fuite ni dérive.

## 2. Objectifs, non-objectifs, anti-objectifs

Objectifs : séquence réelle pilotée par le monde ; réutiliser l'existant (pré-intro, `FacadeLook.build`, `ShotOrder`,
`CarControls`) ; fin de séquence remontée à l'opérateur ; compatible règles 4, 7, 8, 11 de `CLAUDE.md`.

Non-objectifs : changement de DJ (projet 3) ; nouveau mode d'affichage ; refonte du réalisateur ; reprise sur plantage
au milieu d'une séquence (on repart en `Waiting`).

Anti-objectifs (échec même si « ça marche ») : un flash plein écran qui ne passe pas par le limiteur ; un chargement
d'asset ou une compilation de shader déclenchés par `Démarrer` (premier clic saccadé) ; une allocation par image ; un
immeuble dont la base remonte au-dessus de la route (règle mémoire « bâtiments profonds sous la route »).

## 3. Architecture

Sens des dépendances respecté : `cf-operator` ne dépend pas de `cf-world-neon`. Le contrat vit dans **`cf-render`**
(déjà partagé), sur le modèle de `DistrictOrder` / `DistrictState`.

### 3.1 Contrat (cf-render, nouveau module `run/`)

- `RunOrder` (message) : `Start`, `Stop`. Envoyé par l'opérateur.
- `RunPhase` (ressource, **propriété du monde**) : `Waiting | Starting | Running | Stopping`, plus `progress` 0..1 de la
  phase en cours. Valeur par défaut `Running` quand aucun séquenceur n'est installé : exemples, outils et tests existants
  ne changent pas. Le binaire `cyber-funk` installe le séquenceur et initialise `Waiting`.
- `RunGate` : `fn show_may_run(phase)`, vrai en `Starting` (à partir de la phase « intro DJ »), `Running`, `Stopping`.
  Se combine avec `warmup::show_may_run` pour porter `run_show` / `publish_show_fx`.
- Le séquenceur ignore tout `RunOrder` reçu pendant `Starting` / `Stopping` (le bouton est déjà grisé : ceinture et
  bretelles). **Décision : pas d'annulation d'une séquence en cours.**

### 3.2 Séquenceur (cf-world-neon, nouveau module `run/`, machine pure sans Bevy comme `intro/state.rs`)

`RunSequencer::update(order, dt) -> RunOutput`. `RunOutput` = des **étapes** continues 0..1, étendues par rapport à
`IntroStages` : `road, lights, build, windows, sky, sign, drive` (existantes) + `collapse` (nouveau, immeubles qui
rétrécissent), `car_presence` (nouveau, 0 = éteinte et repliée, 1 = pleine), `music` (volume 0..1), `camera`
(`Waiting` ou `Director`). Starting joue la frise de `intro/timeline.rs` à **durée fixe** (pas d'accélération par
l'énergie : l'opérateur doit pouvoir planifier). Stopping joue la même frise **à l'envers** pour la route/ciel, avec les
courbes propres de §4.

Un système `drive_run` (`CfSet::Simulate`, avant `drive_intro`) applique `RunOutput` aux mêmes cibles que `drive_intro`
(`RoadLook.draw`, `FacadeLook.build/collapse`, `SkyLook.reveal`, `DjSignIntro`, `CarControls`, `PilotSettings`,
`TrafficDirector`), avec la même règle « n'écrire que si la valeur change ».

**Pré-intro de niveau 0 existante** : `drive_intro` passe `level: None` quand un séquenceur est installé. La
construction du monde est désormais portée par `Starting` ; sans cela, l'arrivée au niveau 0 du show relancerait une
seconde construction. La logique d'attente musicale de `cf-show` (`MusicWait`, 45 s) reste inchangée.

### 3.3 Façade : nouvel uniforme `collapse`

- `FacadeLook.collapse` (0..1), empaqueté dans un champ libre de `FacadeParams` (à choisir au plan ; le prépasse
  `facade_prepass.wesl` doit refléter exactement le vertex, comme pour `build`).
- Chaque tour a un **décalage propre** tiré de sa graine (`mesh_tag`) : `t_i = clamp(collapse × 1.6 − off_i × 0.6, 0, 1)`.
  C'est la désynchronisation : les tours ne tombent pas ensemble.
- Échelle verticale `s_i = 1 − ease(t_i)`, **appliquée autour de la base enfouie** (`−BASE_DEPTH_M`), jamais autour
  de `y = 0` : la base reste sous la route, la tour s'enfonce.
- Style cyberpunk : pendant `t_i ∈ (0,1)`, l'échelle est **quantifiée** en 6–8 paliers à ~8 Hz avec un décalage
  horizontal de tranche (glitch) de quelques dizaines de cm ; amplitude réduite à 0 si `reduce_motion`.
- Apparition (Matrix) : on garde le front `build` existant (lignes de coupe + balayage) et on ajoute le même
  décalage par tour (`off_i × 0.25`) pour que les tours s'allument en cascade et non en mur.

### 3.4 Voiture

- Elle existe dès `Startup` (jamais de spawn/despawn : elle porte `StreamFocus`). `car_presence` pilote : `Visibility`
  (masquée à 0), échelle verticale de la racine du rig (effet télé : écrasement en ligne puis point, et inverse à
  l'apparition), `light_scale` (phares qui clignotent puis s'allument).
- Arrêt : `PilotSettings.speed_cap_mps` descend en rampe vers 0 (décélération douce, ≤ 4 m/s², confort visuel), puis
  `car_presence` s'éteint quand la vitesse < 0,5 m/s.
- Le point lumineux final de la télé et l'éclat d'apparition passent par `ShowFxInput` → `FlashLimiter` (règle 7), pic
  faible, jamais par une écriture directe.

### 3.5 Caméra d'attente (cf-cinema)

- Nouveau plan `waiting` (id stable `"waiting"`), fichier `shots/waiting.rs` : bas sur la route, regard vers l'horizon
  et le point de départ de la voiture, dérive lente en boucle (≈ 60 s, aller-retour sinusoïdal de ±6 m latéral, ±0,4 m
  vertical, respiration d'angle ≤ 1°). Aucun cut, aucune dépendance à la musique (silence = rien ne bouge d'autre).
- Le séquenceur envoie `ShotOrder::Hold("waiting")` en `Waiting`, et `Release` quand la voiture est apparue (début des
  immeubles). Au retour (`Stopping`), il refait `Hold("waiting")` avec une **transition douce** : à vérifier si `Hold`
  accepte un `TransitionKind`, sinon ajouter une variante `HoldWith(id, TransitionKind)` (changement minimal).

### 3.6 Audio

- `main.rs` : `AudioConfig.autoplay = false`. `Start` envoie `AudioCommand::DemoPlay` au début de `Starting`, avec
  une montée de volume de 4 s ; `Stop` descend le volume sur toute la durée de `Stopping` puis `DemoPause`. **Décision :
  la musique est liée à Démarrer/Arrêter** (en `Waiting`, silence). La source micro n'est pas concernée (non
  pausable) ; le show reste simplement non lancé.
- Le volume de séquence se multiplie avec le volume réglé par l'opérateur, il ne l'écrase pas.

### 3.7 Opérateur

- `ShowRunState.press()` n'applique plus la bascule : il émet `RunOrder::Start|Stop`. `ShowRunState.run` devient le
  miroir de `RunPhase` (système de synchro en début de `Update`). `button_enabled`, libellés (`cockpit/center.rs`) et
  `mode.rs` restent tels quels. `elapsed_s` reste calculé depuis l'entrée en `Starting`.
- À l'entrée en `Waiting`, le séquenceur remet à zéro le runtime du show (`reset_show`), la circulation et les effets
  d'événements, pour que chaque `Démarrer` soit identique au premier.

## 4. Chronologie (mes choix, tout dans `run/timeline.rs`)

| Phase | Durée | Déroulé |
|---|---|---|
| Starting | **18 s** | 0–2,5 s voiture se matérialise, phares clignotent puis s'allument, route tracée près de la caméra ; musique en fondu (4 s). 2,5–11 s immeubles en cascade, voiture accélère 3 → 16 m/s. 7–14 s fenêtres (par vagues sur le front existant), ciel. 13–18 s intro DJ : enseigne lettre par lettre, carte de titre (le show s'arme à 13 s). Fin : `Running`. |
| Stopping | **9 s** | 0–2 s fenêtres s'éteignent d'avant en arrière. 0–5 s voiture freine jusqu'à l'arrêt. 1–6,5 s immeubles désynchronisés rétrécissent. 5–6,5 s extinction « télé » de la voiture, phares coupés. 6–9 s caméra revient au plan d'attente, route et ciel retombent à leur état d'attente. Musique en fondu sur 8 s. Fin : `Waiting`. |
| Waiting | ∞ | route à 25 %, ciel à 30 %, tout le reste à 0. |

Sécurité photosensible : aucune variation de luminance plein écran > 3 Hz ; fenêtres : rampes ≥ 0,3 s ; glitch de
tour = géométrique (pas de flash). `reduce_motion` supprime le glitch et allonge les rampes ×1,5.

## 5. Contraintes

- Règle 8 : tout préchargé (voiture, shaders, plan). Un test d'assertion vérifie qu'aucun asset n'est demandé après
  `Startup`. Règle 4 : aucune allocation par image (étapes = scalaires, pas de `Vec`).
- Règle 11 : fichiers < 400 lignes (`run/` découpé : `mod.rs`, `machine.rs`, `timeline.rs`, `apply.rs`, `car_fx.rs`).
- Commentaires en français, identifiants en anglais.
- Pas de nouveau `unwrap`/`expect` hors tests.

## 6. Tests

- Machine pure : transitions (`Start` depuis `Waiting` seulement, `Stop` depuis `Running` seulement, ordres ignorés
  en transitoire), durées exactes, monotonie des étapes, retour exact à l'état d'attente, répétabilité sur 100 cycles
  (état identique bit à bit), pas de NaN.
- Façade : test pur du décalage par tour et de l'échelle autour de la base enfouie (la base ne dépasse jamais
  `−BASE_DEPTH_M`).
- Limiteur : test qu'aucune étape ne produit un delta de luminance > seuil sur une image.
- Opérateur : `press` émet le bon ordre ; l'état suit `RunPhase` ; bouton grisé en transitoire.
- Hors périmètre vérifiable ici : rendu réel et ressenti (pas d'écran) ; à dire dans le compte rendu final.

## 7. Laissé au constructeur

Emplacement exact des bits libres dans `FacadeParams`, forme précise des courbes de `ease`, choix `Hold`/`HoldWith`,
détail du gating de la circulation et des événements en `Waiting` (à lire dans `traffic/director.rs`, `stunts/`).
