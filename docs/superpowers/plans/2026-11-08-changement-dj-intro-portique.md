# Intro au portique et changement de DJ en voiture : plan d'implémentation

> **Pour les agents :** sous-compétence requise : `superpowers:subagent-driven-development` (recommandé) ou
> `superpowers:executing-plans`. Les étapes utilisent des cases `- [ ]`.

**Objectif :** refaire l'intro (voiture, portique au nom du DJ, puis quartier) et ajouter un changement de DJ spectaculaire
(repli du quartier, 2e voiture qui double, extinction de l'ancienne, portique, reconstruction aux couleurs du nouveau DJ).

**Architecture :** on prolonge le séquenceur pur du projet 2 (`cf-world-neon/src/run/`) d'une phase `Handover` et d'étapes
nouvelles (`gantry`, `double`). Le contrat reste dans `cf-render`. Le chef du spectacle (`cf-show`) garde la passation
(`ArmDj` + `NextDj`, `ShowState.slot` = le DJ actuel) : le monde la déclenche au bon moment, il n'a pas son propre pointeur.

**Pile :** Rust 2024, Bevy 0.20, `bevy_egui`, `cf-show` (RON), `insta`, `proptest`.

**Spec :** `docs/superpowers/specs/2026-11-08-changement-dj-intro-portique-design.md` (lire aussi §6 et §6 bis).

## Écarts avec la spec, constatés dans le code (à signaler dans la PR)

1. **La voiture est un singleton** (`Car`, `CarControls`, `CarState`, pilote, `StreamFocus`). La spec §6 prévoyait deux
   rigs qui échangent leurs rôles. Choix retenu, plus sûr : la voiture **réelle** (pilotée, qui porte `StreamFocus`)
   devient la **nouvelle** voiture ; l'**ancienne** est un **double visuel scripté** (sans pilote, sans `StreamFocus`),
   posé à l'identique au début de la passation (échange invisible). Le focus de streaming ne bouge donc jamais.
2. **La passation du show existe déjà** (`HandoverPhase::{Downbeat, Fade}`, 6 à 10 s de fondu, attente de temps fort ≤ 4 s,
   `Action::ArmDj` / `NextDj`, `cf-operator/src/orders.rs`). Le fondu n'est pas instantané : on le lance **au début du
   repli** pour qu'il soit fini quand le quartier est caché (voir `HANDOVER_FIRE_S`).
3. L'ordre de la liste et le DJ désigné existent (`dj_list.rs`, `ShowState.slot`, `armed_slot`) : on les réutilise.

## Contraintes globales

- Règles `CLAUDE.md` 4 (zéro allocation par image), 6, 7 (limiteur), 8 (rien chargé en show), 11 (< 400 lignes), commentaires
  en français, identifiants en anglais, aucun `unwrap` hors tests. Avant chaque commit : `cargo fmt --all`, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace`.
- `cf-operator` ne dépend pas de `cf-world-neon`. Sillage garde ses noms (`ShowState`, `Gates`…).
- Bâtiments : base à `-BASE_DEPTH_M`, jamais à `y = 0`. Aucune variation de luminance plein écran > 3 Hz.
- Durées dans `run/timeline.rs` seulement. Intro : 12 s voiture, 5 s portique, 12 s quartier = **`START_S = 29`**.
  Passation : **`HANDOVER_S = 27`**. `ARM_AT` = début du portique (12/29).

## Points de vigilance (chacun a un test, dans la tâche indiquée)

- Deux `Handover` ou un `Stop` pendant une passation : ordre ignoré (T3). Passation vers le DJ déjà actuel : ignorée (T3).
- `Start` après plusieurs passations puis `Stop` : repart avec le DJ actuel, pas le premier (T7).
- Dernier DJ : « DJ suivant » grisé, aucun ordre émis (T7). Liste à un seul DJ : pas de passation.
- DJ au nom vide ou très long : le portique ne déborde pas, nom vide = portique sans texte (T4).
- Deux DJs consécutifs avec la même palette demandée à la main : autorisé (le choix manuel l'emporte) (T5).
- `reduce_motion` : plus de clignotement ni de glitch, rampes ×1,5 (T2, T6).

---

### Tâche 1 : le contrat `cf-render`

**Fichiers :** modifier `crates/cf-render/src/run/mod.rs`, `crates/cf-render/src/lib.rs:71`.

**Interfaces — produit :**
- `RunOrder::Handover { slot: Option<usize> }` (`None` = DJ suivant dans la liste, `Some(i)` = ce DJ).
- `Phase::Handover`, traité comme `Starting`/`Stopping` pour `show_armed` (vrai).
- `RunHandover(Option<usize>)` : ressource écrite par le monde, le rang du DJ visé pendant la passation.

- [ ] Écrire les tests : `show_armed` vrai en `Handover` quelle que soit l'avancée ; `RunOrder::Handover` est `Copy` et `Eq`.
- [ ] Lancer `cargo test -p cf-render run::` : échec. Ajouter les variantes et la ressource ; relancer : réussite.
- [ ] Corriger les `match` devenus non exhaustifs dans tout le workspace (`cargo check --workspace`), sans changer de comportement.
- [ ] Commit `[run] Contrat : RunOrder::Handover et Phase::Handover`.

### Tâche 2 : frise et étapes (pur, sans Bevy)

**Fichiers :** modifier `crates/cf-world-neon/src/run/timeline.rs` ; créer `crates/cf-world-neon/src/run/handover.rs`.

**Interfaces — produit :**
- `RunStages` gagne `gantry: f32` (0 absent, 0,5 au-dessus de la voiture, 1 replié) et `name_glitch: f32` (0 stable, 1 plein
  glitch), `collapse` est réutilisé pour le repli.
- `pub struct DoubleStages { pub presence: f32, pub pull_right: f32, pub brake: f32, pub glitch: f32 }` (l'ancienne voiture).
- `pub struct ArrivalStages { pub gap_behind_m: f32, pub lateral_m: f32, pub boost: f32 }` (la voiture réelle qui arrive
  par l'arrière gauche et double).
- `pub fn starting_stages(t_s: f32, reduce_motion: bool) -> RunStages` (nouvel ordre), `pub fn handover_stages(t_s: f32,
  reduce_motion: bool) -> (RunStages, DoubleStages, ArrivalStages)`.
- Constantes : `START_S`, `HANDOVER_S`, `HANDOVER_FIRE_S` (début du repli, 0 s), `HANDOVER_SWAP_S`, `OVERTAKE_S`.

- [ ] Tests (valeurs exactes de la spec) : à `t = 0` la voiture est absente et le quartier à 0 ; `build` reste à 0 jusqu'à
  `t = 17 s` ; `gantry` couvre 12-17 s ; toutes les étapes dans `[0, 1]`, monotones à l'intro ; en passation, `collapse`
  monte avant `OVERTAKE_S`, retombe à 0 puis `build` repart seulement après le portique ; l'échange de voiture
  (`HANDOVER_SWAP_S`) a lieu quand `double.presence == 1` et `arrival` est hors champ ; aucune étape ne varie de plus de
  0,3 par image à 60 Hz sur la luminance (`name_glitch` et `double.glitch` exclus : ils vont au limiteur, T6).
- [ ] Implémenter les fonctions avec `ramp` et `mix` existants. Garder le fichier < 400 lignes (d'où `handover.rs`).
- [ ] Mettre à jour `ARM_AT` dans `cf-render` et le test qui lie `ARM_AT` à `START_S`.
- [ ] Lancer `cargo test -p cf-world-neon run::` : réussite. Commit `[run] Frise : intro portique et passation`.

### Tâche 3 : la machine d'état

**Fichiers :** modifier `crates/cf-world-neon/src/run/machine.rs` (sortir les tests dans `tests.rs` si besoin).

**Interfaces — consomme :** T1, T2. **Produit :** `RunOutput` gagne `double: DoubleStages`, `arrival: ArrivalStages`,
`handover_slot: Option<usize>` ; `RunSequencer::update(order, dt_s, kick)` accepte `RunOrder::Handover` depuis `Running`
seulement ; `RunSequencer::set_slot_info(current: usize, count: usize)` (le monde dit où en est la liste).

- [ ] Tests : `Handover` depuis `Running` entre en `Handover` ; ignoré depuis `Waiting`, `Starting`, `Stopping`, `Handover` ;
  ignoré si `count <= 1` ou si la cible est le DJ actuel ; un `Stop` pendant la passation est ignoré ;
  `Handover { slot: None }` vise `current + 1` et est ignoré au dernier ; fin de passation = retour à `Running` avec
  `RunStages::RUNNING` ; 100 cycles `Start`, 3 `Handover`, `Stop` ramènent bit à bit la même sortie d'attente ; pas de NaN.
- [ ] Implémenter. Lancer les tests : réussite. Commit `[run] Machine : phase Handover`.

### Tâche 4 : le portique

**Fichiers :** créer `crates/cf-world-neon/src/gantry/{mod.rs,rig.rs,systems.rs}` (lire d'abord `djsign/rig.rs`,
`djsign/layout.rs`, `djsign/glyphs.rs`, `djsign/anim.rs` : on réutilise les lettres et leur mise en page).

**Interfaces — produit :** `GantryPlugin`, composant `Gantry`, ressource `GantrySettings { ahead_m: f32, height_m: f32,
span_m: f32 }`, système `drive_gantry` (`CfSet::Simulate`) qui lit `RunStatus` (étapes `gantry`, `name_glitch`) et le nom
du DJ visé.

- [ ] Tests purs : le nom vide donne un portique sans lettres ; un nom de 40 caractères tient dans `span_m` (échelle
  réduite, jamais de débordement) ; les pieds sont à `-BASE_DEPTH_M` ; `gantry = 1` replie le portique sous la route.
- [ ] Test d'app : le portique est créé **à `Startup`** (règle 8), sans `NoFrustumCulling`, avec un `Aabb` borné ; il est
  placé à `ahead_m` devant la voiture au moment où l'étape `gantry` quitte 0 ; aucun asset demandé après `Startup`.
- [ ] Implémenter : portique métallique (poutre et deux pieds, matériaux déjà préchauffés), nom en néon dans la palette
  courante, apparition holographique brève (géométrique, pas de flash). Brancher `name_glitch` sur `ShowFxInput` (limiteur).
- [ ] Retirer l'appel à l'ancienne enseigne `DjSignIntro` de l'intro (elle reste utilisée hors séquence). Commit
  `[gantry] Portique au nom du DJ`.

### Tâche 5 : palettes de présélection

**Fichiers :** modifier `crates/cf-show/src/model/look.rs` (`PresetTheme`), `assets/themes.ron`,
`crates/cf-render/src/palette/scene.rs` (`PRESETS`, `preset_name`) ; créer `crates/cf-show/src/model/auto_theme.rs`.

**Interfaces — produit :** quatre nouveaux préréglages ou plus (10 au total, les 6 existants inchangés) ;
`pub fn assign_themes(slots: &mut [DjSlot], presets: &[PresetTheme])` : donne un thème aux DJs qui n'en ont pas, jamais le
même que le DJ précédent, ne touche pas aux thèmes choisis à la main.

- [ ] Tests (`proptest`) : pour toute liste de 1 à 12 DJs, deux consécutifs auto-attribués ont des thèmes différents ;
  un thème manuel n'est jamais écrasé ; l'attribution est déterministe (même liste, même résultat) ; l'ancien RON se
  relit à l'identique (`roundtrip.rs`).
- [ ] Appeler `assign_themes` à l'ouverture du show et enregistrer le résultat dans le RON ; ajouter un menu de
  présélections dans la fiche du DJ (`cf-operator/src/tabs/settings/`).
- [ ] Test `insta` : les 10 palettes ont un contraste minimal encre/néon (ratio ≥ 4,5). Commit `[show] Palettes de présélection`.

### Tâche 6 : le double de la voiture et la voiture qui arrive

**Fichiers :** créer `crates/cf-world-neon/src/car/double.rs` ; modifier `car/systems.rs` (`build_car`), `car/mod.rs`.

**Interfaces — produit :** composant `DoubleCar` (racine **sans** `Car` ni `StreamFocus`), système `drive_double`
(`CfSet::Simulate`, après `drive_car`) qui pose le double depuis `RunStatus.double`, et `drive_arrival` qui téléporte la
voiture réelle à `gap_behind_m` derrière et `lateral_m` à gauche de l'ancienne à `HANDOVER_SWAP_S`, la repeint avec la
palette du DJ visé et lui laisse le pilote rattraper (`PilotSettings.speed_cap_mps` relevé par `arrival.boost`).

- [ ] Tests : le double existe dès `Startup`, reste dessiné à `MIN_SCALE` quand il est éteint (pipelines chauds, comme
  `presence.rs`) ; à l'échange, la pose du double égale celle de la voiture réelle à 1 cm près ; la voiture réelle garde son
  `StreamFocus` et son `AbsolutePosition.y == 0` ; la téléportation ne dépasse jamais la distance prévue.
- [ ] Test limiteur : le clignotement du double passe par `ShowFxInput` et `FlashLimiter` ; avec `reduce_motion` il vaut 0.
- [ ] Implémenter en réutilisant `spawn_car` (lire sa signature, `car/spawn.rs:197`) et la peinture de `car/paint.rs`.
  Si le pilote refuse la téléportation (piste de vol, `reset_track`), la documenter dans la PR plutôt que contourner.
- [ ] Commit `[car] Double visuel et voiture qui arrive`.

### Tâche 7 : caméra, application au monde et chef du spectacle

**Fichiers :** créer `crates/cf-cinema/src/shots/handover.rs` (id `"handover"`) ; modifier `run/apply.rs` ; modifier
`crates/cf-render/src/show/mod.rs` et `crates/cf-render/src/sim.rs`.

**Interfaces — produit :** plan `handover` (cadre l'ancienne voiture et la nouvelle, aucun cut) ; dans `drive_run`,
`Phase::Handover` pose `collapse`, `build`, `gantry`, double et arrivée ; à `HANDOVER_FIRE_S`, écrit
`Action::ArmDj(slot)` puis `Action::NextDj` pour le chef du spectacle (même chemin que `cf-operator/src/orders.rs`) ;
`SimControl::request_reset_keeping_slot()` : `Démarrer` ne remet plus le DJ à zéro.

- [ ] Tests : la caméra tient `handover` de l'échange à `OVERTAKE_S` puis `Release` ; le chef du spectacle reçoit les
  deux actions exactement une fois ; `Stop`, `Start` après trois passations repart avec `ShowState.slot` inchangé ;
  désigner le premier DJ à l'arrêt puis `Start` joue le premier ; le fondu de palette est terminé (`settled`) avant
  la première étape `build` de la reconstruction.
- [ ] Implémenter et brancher les cibles ; la circulation et les événements restent coupés pendant la passation.
- [ ] Commit `[run] Passation branchée au monde et au chef du spectacle`.

### Tâche 8 : opérateur

**Fichiers :** modifier `crates/cf-operator/src/run_state.rs`, `cockpit/center.rs`, `cockpit/dj_column.rs`, `mode.rs`.

**Interfaces — consomme :** `RunOrder::Handover`, `Phase::Handover`. **Produit :** bouton « DJ suivant » ; clic sur un DJ
= `RunOrder::Handover { slot: Some(i) }` pendant un set, ou désignation du DJ actuel à l'arrêt (`Resource StartSlot(usize)`
lue par `Démarrer`).

- [ ] Tests : « DJ suivant » émet `Handover { slot: None }` ; grisé en `Starting`/`Stopping`/`Handover` et au dernier
  DJ (aucun ordre émis) ; `ShowRunState.run` suit `Phase::Handover` ; réordonner la liste pendant une passation n'a pas
  d'effet sur celle-ci ; à l'arrêt, un clic désigne le DJ actuel.
- [ ] Implémenter. Lancer `cargo test -p cf-operator`. Commit `[operator] DJ suivant et DJ actuel`.

### Tâche 9 : sécurité, endurance et documentation

**Fichiers :** modifier `crates/cf-world-neon/tests/` (nouveau `handover_soak.rs`), `docs/ARCHITECTURE.md`,
`docs/guide-operateur.md`.

- [ ] Test d'endurance logique : 50 cycles `Start`, 3 passations, `Stop` sans fuite d'entités ni de ressources ; un seul
  `StreamFocus` à tout instant ; aucune allocation par image dans `drive_run`, `drive_double`, `drive_gantry`.
- [ ] Test : aucune base d'immeuble au-dessus de `-BASE_DEPTH_M` pendant le repli et la reconstruction.
- [ ] Documenter la séquence, les boutons et les palettes. Noter dans la PR ce qui n'a pas pu être vérifié (rendu, ressenti :
  pas d'écran). Lancer la suite complète (`fmt`, `clippy`, `nextest`). Commit `[run] Endurance et documentation`.

---

## Revue du plan

- **Couverture de la spec :** intro §3.1 (T2, T3, T7), portique §3.2 (T4), passation §3.3 (T2, T3, T6, T7), opérateur §3.4 (T8),
  palettes §3.5 (T5), précisions §6 et §6 bis (T3, T7, T8) ; contraintes §4 (T9).
- **Cohérence des types :** `RunOrder::Handover`, `Phase::Handover`, `DoubleStages`, `ArrivalStages`, `RunOutput`
  utilisés à l'identique de T1 à T8.
- **Risque principal :** la téléportation de la voiture réelle avec le pilote (T6). Si elle échoue, on s'arrête et on le
  note dans la PR, sans itérer au-delà de deux essais (règle de `CLAUDE.md`).
