# Séquence de démarrage et d'arrêt du show 3D — plan d'implémentation

> **Pour les agents :** exécution inline (`superpowers:executing-plans`), tâche par tâche, cases `- [ ]`.

**Objectif :** Démarrer / Arrêter pilotent réellement le monde 3D (apparition façon Matrix, voiture, désynchronisation,
caméra d'attente, musique), et la fin de séquence remonte à l'opérateur.

**Architecture :** contrat dans `cf-render` (`RunOrder` message, `RunPhase` ressource) ; machine pure `RunSequencer`
dans `cf-world-neon/src/run/` qui rend des étapes continues ; un système applique ces étapes aux cibles que pilote déjà
la pré-intro ; l'opérateur envoie les ordres et reflète la phase.

**Stack :** Rust, Bevy 0.20, WGSL/WESL, `cargo nextest`.

**Spec :** `docs/superpowers/specs/2026-11-08-sequence-demarrage-arret-design.md`

## Contraintes globales

- Règles 🔴 de `CLAUDE.md` : pas de `unwrap/expect` hors tests ; pas d'allocation par image ; fichiers < 400 lignes ;
  flashs via `FlashLimiter` uniquement ; aucun chargement d'asset / shader après `Startup`.
- Commentaires en français, identifiants en anglais.
- Bâtiments : leur base reste enfouie à `-BASE_DEPTH_M` (`scenery/layout.rs`) ; jamais d'échelle autour de `y = 0`.
- **Aucun texte plein écran pour annoncer le DJ** : seulement `djsign` et panneaux néon du monde (spec §8).
- Durées : Starting 18 s, Stopping 9 s (table de constantes unique `run/timeline.rs`).
- Avant chaque commit : itérer avec `-p <crate>` ; `fmt`, `clippy --workspace`, `test --workspace` avant de pousser.
  `CARGO_INCREMENTAL=0`.

## Points de vigilance (testés dans la tâche indiquée)

- Ordre reçu pendant une séquence : ignoré, pas de double séquence (T2).
- 100 cycles Start/Stop : état final identique au départ, pas de NaN, pas de dérive (T2).
- Délai image géant (`dt` de plusieurs secondes après un gel) : la phase finit, ne saute pas d'état (T2).
- Base des tours jamais au-dessus de `-BASE_DEPTH_M` quelle que soit `collapse` (T4).
- Sans séquenceur installé (exemples, tests existants) : le monde se comporte comme avant (T1, T3).
- `reduce_motion` : pas de glitch (T4).

---

### Tâche 1 : contrat `cf-render::run`

**Fichiers :** créer `crates/cf-render/src/run/mod.rs` ; modifier `crates/cf-render/src/lib.rs` (suivre l'export de
`district`, l.52).

**Interfaces — produit :**
- `#[derive(Message)] pub enum RunOrder { Start, Stop }`
- `#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)] pub enum Phase { Waiting, Starting, #[default] Running, Stopping }`
- `#[derive(Resource)] pub struct RunPhase { pub phase: Phase, pub progress: f32 }` ; `Default` = `Running`, 1.0
  (sans séquenceur, rien ne change).
- `pub fn show_armed(phase: Option<Res<RunPhase>>) -> bool` (condition `run_if`) : vrai si pas de ressource, ou
  phase `Running`/`Stopping`, ou `Starting` avec `progress >= ARM_AT` (`pub const ARM_AT: f32 = 13.0 / 18.0`).
- `pub struct RunPlugin` : `add_message::<RunOrder>()` et `init_resource::<RunPhase>()` (idempotent).

- [ ] Test d'abord : `default_phase_is_running`, `show_armed_without_resource_is_true`,
  `show_armed_false_while_waiting`, `show_armed_in_starting_only_after_arm_point`.
- [ ] Lancer : `cargo test -p cf-render run::` — échec attendu (module absent).
- [ ] Implémenter le module, exporter depuis `lib.rs`.
- [ ] Brancher `show_armed` sur `run_show` / `publish_show_fx` (`cf-render/src/show/mod.rs` ~l.183) en combinaison
  avec `warmup::show_may_run` (`.run_if(show_may_run.and(show_armed))`). Ajouter `RunPlugin` aux plugins de `cf-render`.
- [ ] Lancer `cargo test -p cf-render` : tout vert (les tests existants n'installent pas de séquenceur).
- [ ] Commit `[run] Contrat RunOrder / RunPhase dans cf-render`.

### Tâche 2 : machine pure `RunSequencer`

**Fichiers :** créer `crates/cf-world-neon/src/run/{mod.rs,machine.rs,timeline.rs}` ; modifier
`crates/cf-world-neon/src/lib.rs` (`pub mod run;`).

**Interfaces — consomme :** `cf_render::Phase`, `cf_world_neon::intro::{ramp, speed_cap}`.
**Produit :**
- `pub struct RunStages { road, lights, build, windows, sky, sign, drive, collapse, car_presence, music: f32,
  camera_waiting: bool }` (`Copy`, `PartialEq`) ; `RunStages::WAITING` et `RunStages::RUNNING`.
- `pub struct RunSequencer` ; `RunSequencer::new() -> Self` (démarre en `Waiting`) ;
  `update(&mut self, order: Option<RunOrderKind>, dt_s: f32) -> RunOutput` ;
  `RunOutput { phase: Phase, progress: f32, stages: RunStages, speed_cap_mps: Option<f32>, front: Option<WindowFront> }`.
- `timeline.rs` : constantes `START_S = 18.0`, `STOP_S = 9.0`, bornes de chaque étape (table du spec §4),
  `pub fn starting_stages(t_s) -> RunStages`, `pub fn stopping_stages(t_s) -> RunStages`.
- Le front des fenêtres réutilise `intro::PreIntro::front` : extraire cette méthode en fonction libre
  `intro::window_front(...)` si nécessaire, sans changer la pré-intro.

- [ ] Tests d'abord, nommés : `start_from_waiting_enters_starting`, `stop_from_waiting_is_ignored`,
  `orders_ignored_during_starting`, `orders_ignored_during_stopping`, `starting_ends_in_running_after_18s`,
  `stopping_ends_in_waiting_after_9s`, `waiting_stages_match_spec_table` (route 0,25, ciel 0,3, le reste 0),
  `running_stages_are_all_in_place`, `stages_are_monotonic_while_starting`, `no_nan_over_100_cycles`,
  `hundred_cycles_return_to_identical_waiting_state`, `huge_dt_finishes_phase_without_skipping`.
- [ ] Lancer, vérifier l'échec, implémenter, relancer : `cargo test -p cf-world-neon run::`.
- [ ] Arrêt : `speed_cap_mps` décroît d'au plus 4 m/s² ; test `braking_never_exceeds_4_mps2`.
- [ ] Commit.

### Tâche 3 : application au monde

**Fichiers :** créer `crates/cf-world-neon/src/run/apply.rs`, `plugin.rs` ; modifier `intro/mod.rs`, `lib.rs`.

**Interfaces — consomme :** `RunSequencer`, cibles de `intro::Targets` (rendre `Targets` `pub(crate)`).
**Produit :** `pub struct RunPlugin` (côté monde) : système `drive_run` en `CfSet::Simulate`, **avant** `drive_intro` ;
lit `MessageReader<cf_render::RunOrder>`, écrit `cf_render::RunPhase`.

- [ ] Test d'intégration (App Bevy minimale, motif de `automation_tests.rs`) : `start_order_moves_phase_to_running`
  après 18 s simulées ; `stop_order_returns_to_waiting`.
- [ ] `drive_run` applique `RunStages` aux mêmes cibles que `drive_intro` (`RoadLook.draw`, `FacadeLook.build`,
  `SkyLook.reveal`, `DjSignIntro`, `CarControls.light_scale`, `PilotSettings.speed_cap_mps`, power front), en
  n'écrivant que si la valeur change (`set`).
- [ ] `drive_intro` reçoit `level: None` quand `RunPhase` est présente (la pré-intro de niveau 0 ne rejoue pas).
  Test `intro_is_inert_when_sequencer_installed`.
- [ ] En entrée de `Waiting` : remise à zéro du runtime du show (`reset_show`), de la circulation et des effets
  d'événements (lire `traffic/director.rs`, `stunts/` ; trouver l'API de remise à zéro existante, sinon la créer
  minimale). Test : après un cycle, `CurrentShow` et le trafic sont à l'état initial.
- [ ] En `Waiting`, la circulation et les cascades d'événements ne produisent rien (porte `RunPhase` dans leur
  `run_if`).
- [ ] Commit.

### Tâche 4 : immeubles — apparition en cascade et désynchronisation (shader)

**Fichiers :** modifier `crates/cf-world-neon/src/facade/{mod.rs,systems.rs}`, `assets/shaders/cf-world-neon/facade.wesl`,
`facade_prepass.wesl` ; créer `facade/collapse.rs` (fonctions pures + tests).

**Interfaces — produit :** `FacadeLook.collapse: f32` (0 par défaut, ajouté à `Default`) ;
`pub fn tower_offset(seed: u32) -> f32` (0..1, déterministe) ;
`pub fn tower_scale(collapse: f32, offset: f32, time_s: f32, glitch: bool) -> f32` (1 → 0, quantifié par paliers si
`glitch`) ; `pub fn scaled_y(y: f32, scale: f32, base_y: f32) -> f32` (échelle autour de `base_y`).

- [ ] Tests purs d'abord : `scale_is_one_at_zero_collapse`, `scale_is_zero_at_full_collapse_for_every_offset`,
  `offsets_differ_between_seeds`, `base_never_rises_above_buried_base` (`scaled_y(-BASE_DEPTH_M…)` reste ≤
  `-BASE_DEPTH_M`), `no_glitch_when_reduce_motion` (valeurs continues), `quantised_steps_when_glitch`.
- [ ] Empaqueter `collapse` dans un champ libre de `FacadeParams` (l'inspecter ; ne pas agrandir l'uniforme si un
  `w` est libre) ; vertex : `wp.y = base + (wp.y - base) * tower_scale(...)` ; décalage de tranche horizontal en
  `glitch`. Le prépasse reproduit exactement le même calcul.
- [ ] Apparition en cascade : le décalage par tour (`off × 0.25`) s'ajoute au front `build`, côté fragment et prépasse.
- [ ] Vérifier la compilation des shaders (`cargo test -p cf-world-neon` couvre `naga`/préchargement) et qu'aucun
  shader n'est compilé après `Startup` (`warmup` reste inchangé).
- [ ] `drive_run` écrit `FacadeLook.collapse`. Commit.

### Tâche 5 : voiture — présence, freinage, extinction « télé »

**Fichiers :** modifier `car/systems.rs` (`CarControls.presence: f32`, défaut 1.0) ; créer `car/presence.rs`.

- [ ] Tests purs : `presence_zero_hides_car`, `tv_squash_profile` (écrasement vertical en ligne puis point, bornes
  [0,1], monotone), `presence_one_is_identity`.
- [ ] Système `apply_presence` : `Visibility::Hidden` à 0 ; échelle Y de la racine du rig selon le profil ; pas de
  spawn/despawn (la voiture porte `StreamFocus`). Aucune allocation par image.
- [ ] Éclats (apparition, point final) : émis en `ShowFxInput` → `FlashLimiter` (jamais d'écriture directe) ; test que
  la demande reste sous `MAX_FLASH_PEAK`.
- [ ] `drive_run` écrit `CarControls.presence` ; arrêt : `presence` s'éteint quand vitesse < 0,5 m/s.
- [ ] Commit.

### Tâche 6 : caméra d'attente (cf-cinema)

**Fichiers :** créer `crates/cf-cinema/src/shots/waiting.rs` ; modifier `shots/mod.rs` (catalogue), `director/hold.rs`
(si `Hold` ne prend pas de transition), `plugin.rs`.

**Interfaces :** id `"waiting"` ; `ShotOrder::Hold("waiting")` / `Release` (existants).
Variante éventuelle `ShotOrder::HoldWith(&'static str, TransitionKind)` seulement si `hold_shot` ne permet pas la
transition douce au retour.

- [ ] Tests (motif de `shots/tests.rs`) : `waiting_pose_is_low_over_road` (hauteur < 4 m, regard vers −z / horizon),
  `waiting_loops_seamlessly` (pose en t = 0 et t = période égales), `waiting_never_cuts`,
  `waiting_amplitudes_bounded` (±6 m latéral, ±0,4 m vertical, ≤ 1° d'angle).
- [ ] Implémenter le plan : période 60 s, sinus lent ; aucune dépendance à la musique.
- [ ] `drive_run` émet `ShotOrder::Hold("waiting")` en `Waiting`, `Release` dès que la voiture est apparue
  (début de l'étape `build`), puis `Hold("waiting")` à nouveau au retour de `Stopping` (étape caméra à 6 s).
  Test d'ordres : séquence exacte émise sur un cycle complet.
- [ ] Le plan `waiting` ne doit pas être choisi par le montage automatique (exclu du tirage). Test.
- [ ] Commit.

### Tâche 7 : audio lié à Démarrer / Arrêter

**Fichiers :** `crates/cyber-funk/src/main.rs:218-228` (`autoplay: false`) ; `crates/cf-world-neon/src/run/audio.rs`
(commandes) ; lire `crates/cf-render/src/audio/{types,runtime}.rs`.

- [ ] Test : au début de `Starting` un `AudioCommand::DemoPlay` est émis une seule fois ; à la fin de `Stopping`,
  `DemoPause` une seule fois ; le volume de séquence (`music` 0..1) se **multiplie** au volume opérateur
  (ne l'écrase pas — lire comment `DemoVolume` est stocké, ajouter un facteur `sequence_gain` dans le lecteur si
  nécessaire).
- [ ] Montée 4 s, descente sur 8 s. Source micro : aucun contrôle.
- [ ] Commit.

### Tâche 8 : annonce du DJ uniquement dans le monde

**Fichiers :** `crates/cf-render/src/titles/plugin.rs` (:181-198), `crates/cf-world-neon/src/djsign/`,
`signs/ads.rs`.

- [ ] Test : pendant `Starting` aucune `TitleCard` de type `Dj` n'est créée (`ActiveTitle` reste vide) ; l'enseigne
  `djsign` reçoit la progression des lettres (`DjSignIntro`).
- [ ] Neutraliser la carte `Dj` (garder `Level`/`Drop` si utilisées ailleurs ; vérifier leur usage et le noter dans
  le commit). Si des panneaux de route/immeubles peuvent afficher le nom du DJ via `signs/ads.rs`, l'y brancher ;
  sinon seule l'enseigne `djsign` annonce et on le note dans « Écarts ».
- [ ] Commit.

### Tâche 9 : opérateur

**Fichiers :** `crates/cf-operator/src/run_state.rs`, `ui.rs:273`, `lib.rs`, `cockpit/center.rs` (libellés déjà là).

- [ ] Tests : `press_in_waiting_sends_start`, `press_in_running_sends_stop`, `press_during_transition_sends_nothing`,
  `state_mirrors_run_phase` (la ressource `ShowRunState.run` suit `RunPhase`), `button_disabled_in_transitions`.
- [ ] `ShowRunState::press` n'applique plus la bascule : il retourne l'ordre à émettre (ou prend un
  `MessageWriter<RunOrder>`) ; un système de synchro recopie `RunPhase` dans `ShowRunState.run` (avec
  `started_at_s` posé à l'entrée en `Starting`). Sans séquenceur (opérateur seul, tests de la crate) : repli sur
  l'ancien comportement direct, conservé derrière l'absence de `RunPhase` — vérifier que les 170 tests existants
  passent.
- [ ] Commit.

### Tâche 10 : câblage du binaire et vérification

**Fichiers :** `crates/cyber-funk/src/{main.rs,wiring.rs}`, `docs/ARCHITECTURE.md` (une section courte).

- [ ] Installer `cf_world_neon::run::RunPlugin` ; initialiser `RunPhase` à `Waiting` (le binaire seul).
- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo nextest run --workspace` : tout vert.
- [ ] Soak : test 100 cycles sur l'App complète si le harnais `soak.rs` le permet.
- [ ] Lancer l'application si possible et capturer ; sinon consigner ce qui n'a pas pu être vérifié (rendu, ressenti).
- [ ] Revue finale (`requesting-code-review`), corrections, push sur `main`.

---

## Auto-revue

Couverture du spec : §3.1 → T1 ; §3.2 → T2, T3 ; §3.3 → T4 ; §3.4 → T5 ; §3.5 → T6 ; §3.6 → T7 ; §3.7 → T9 ; §8 → T8 ;
§6 tests → répartis ; câblage → T10. Écarts connus : détails des bits d'`FacadeParams` et de l'API de remise à zéro du
trafic laissés à l'implémentation (spec §7).
