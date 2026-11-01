# Moteur de quartiers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** La ville change de quartier toute seule (centre néon, banlieue, résidentiel) selon le son, avec un dégradé de 10 s maximum et un étalonnage de couleur modéré par-dessus le thème du show, plus un panneau F4 refait pour forcer un quartier ou une transition.

**Architecture:** Un gouverneur pur (`cf-show`, sans Bevy) décide quand et vers quoi changer à partir d'un instantané audio. Un plan de quartiers (fonction pure de l'index de rangée, dans `SceneryConfig`) pilote `Row::generate`. Un étalonnage borné s'applique en sortie de `drive_palette`. La colle Bevy vit dans `cf-world-neon`, l'interface dans `cf-operator`.

**Tech Stack:** Rust 2024, Bevy 0.20, bevy_egui, RON, `cargo nextest`.

**Spec:** `docs/superpowers/specs/2026-11-01-quartiers-moteur-design.md` (lire aussi la section « Précisions après relecture » et « Décision de l'utilisateur sur la palette »).

## Global Constraints

- Règle 5 : `cf-show` ne dépend pas de Bevy. Règle 4 : pas d'allocation par image en régime établi. Règle 7 : tout flash passe par le limiteur ; ce chantier n'ajoute aucune vague de lumière. Règle 8 : pas de variante de shader ni de compilation pendant un show. Règle 11 : fichier ≤ 400 lignes (hors tests/données).
- Commentaires et docs en français, identifiants en anglais.
- Le golden `crates/cf-world-neon/tests/golden/scenery.json` reste identique pour le quartier par défaut (centre néon sur toute la route).
- Le thème du show (changement de DJ) reste la base de la palette ; l'étalonnage de quartier le module, de façon bornée (teinte ≈ ±30°, à régler), toujours moins drastique qu'un changement de thème.
- Dégradé de transition : 10 s maximum. Temps minimum dans un quartier 150 s, maximum 8 min. Jamais le même quartier deux fois de suite. Aucun changement en section Peak, avec drop imminent, ni dans les 8 mesures (16 s sans BPM) après un drop.
- Entièrement automatique. Seul le debug (F4, Préparation uniquement) force ; rien n'est sauvegardé dans les looks ni d'une session à l'autre (gel et forçage).
- Les 6 transitions autres que le dégradé retombent sur le dégradé et F4 le dit : « (repli : dégradé) ».
- Avant chaque commit : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, tests verts (itérer par `-p <crate>`, workspace une fois avant de pousser). Compiler avec `CARGO_INCREMENTAL=0`.

## Review Focus

- Un drop ou une section Peak qui arrive pendant la fenêtre ouverte : le gouverneur ne doit jamais lancer de changement ; une transition déjà commencée finit en ≤ 10 s.
- Le BPM absent ou `AudioFeatures` absent : la fenêtre « 8 mesures » retombe sur 16 s ; sans show, le gouverneur ne propose rien.
- Le plan de quartiers ne contredit jamais les rangées déjà générées (marge de 2 rangées minimum devant la dernière générée ; la voiture qui accélère ne réécrit pas le passé).
- Deux exécutions avec la même graine donnent les mêmes quartiers, variantes et mélanges.
- Un changement de DJ en plein quartier garde l'étalonnage et ne produit pas un saut de couleur supérieur au fondu du show.
- Un forçage F4 pendant une transition la remplace sans à-coup ; geler l'automatique n'empêche pas le forçage.

---

### Task 1: Gouverneur de quartiers (cf-show, sans Bevy)

**Files:**
- Create: `crates/cf-show/src/district/mod.rs` (types), `crates/cf-show/src/district/governor.rs` (règles), `crates/cf-show/src/district/tests.rs`
- Modify: `crates/cf-show/src/lib.rs` (déclarer `pub mod district;`)

**Interfaces:**
- Produces (utilisés par les tâches 2 à 5) :
  - `pub enum DistrictKind { Neon, Suburb, Residential }` avec `ALL: [DistrictKind; 3]`, `fn label(self) -> &'static str`, `fn slug(self) -> &'static str` (`"neon"`, `"suburb"`, `"residential"`).
  - `pub struct DistrictId { pub kind: DistrictKind, pub variant: u8 }`.
  - `pub enum TransitionKind { Gradient, Fog, Buffer, Bridge, Shrink, Blackout, Interchange }` avec `fn implemented(self) -> bool` (vrai seulement pour `Gradient`) et `fn label(self)`.
  - `pub struct GovernorSnapshot { pub now_s: f64, pub section: Section, pub section_changed: bool, pub intensity: f32, pub bass: f32, pub high: f32, pub bpm: Option<f32>, pub drop_imminent: bool, pub drop_now: bool, pub event_ended: bool, pub show_present: bool }` (`Section` vient de `cf-analysis`, déjà dépendance de `cf-show`).
  - `pub enum Decision { Stay { reason: StayReason }, Change { to: DistrictId, transition: TransitionKind } }` et `pub enum StayReason { NoShow, TooSoon, PeakSection, DropImminent, AfterDrop, NoWindow, Transitioning, Frozen }` (affichés en F4 via `fn label`).
  - `pub struct Governor` : `fn new(seed: u64, variants_per_kind: [u8; 3]) -> Self`, `fn tick(&mut self, snap: &GovernorSnapshot) -> Decision`, `fn current(&self) -> DistrictId`, `fn history(&self) -> &[DistrictId]` (5 derniers), `fn set_frozen(&mut self, frozen: bool)`, `fn force(&mut self, to: DistrictId, transition: TransitionKind, now_s: f64)` (contourne les règles ; remplace une transition en cours), `fn transition_done(&mut self, now_s: f64)` (appelé par la colle quand le dégradé est fini ; sinon fin automatique à `TRANSITION_MAX_S`).
  - Constantes publiques : `MIN_DWELL_S = 150.0`, `MAX_DWELL_S = 480.0`, `TRANSITION_MAX_S = 10.0`, `AFTER_DROP_BEATS = 8`, `AFTER_DROP_FALLBACK_S = 16.0`.

- [ ] **Step 1: Write the failing tests** dans `district/tests.rs`, avec des séquences d'instantanés construites à la main (helper `snap(now_s)` et des modificateurs) :
  - `jamais_en_peak` : pendant 20 minutes simulées avec `section = Peak` en continu, aucune `Change`.
  - `jamais_avec_drop_imminent` et `jamais_apres_un_drop` : après `drop_now`, aucun `Change` avant 8 mesures à 128 BPM (3,75 s par mesure de 4 temps, soit 30 s), ni avant 16 s sans BPM.
  - `temps_minimum_respecte` : fenêtre ouverte (Calm) dès t = 0 mais aucun `Change` avant 150 s ; un `Change` à t ≥ 150 s.
  - `temps_maximum` : en `Groove` sans `section_changed`, un `Change` survient dès que 480 s sont passées et que la fenêtre est ouverte.
  - `jamais_deux_fois_le_meme` : sur 200 changements seedés, `to.kind != current.kind` et `to != current` ; si le choix de types est épuisé par l'historique, le type peut se répéter mais `variant` diffère de la précédente.
  - `reproductible` : même graine, mêmes instantanés, mêmes décisions ; deux graines différentes donnent des suites différentes.
  - `sans_show_ne_propose_rien` : `show_present = false` donne `Stay { reason: NoShow }`.
  - `force_remplace_la_transition` et `gel_bloque_l_automatique_pas_le_force`.
  - `penchant_sonore` : sur 2 000 tirages seedés, intensité haute favorise `Neon`, basses fortes et aigus faibles favorisent `Suburb`, calme favorise `Residential` (proportion strictement supérieure à un tiers pour le type favorisé) tout en laissant les deux autres possibles (≥ 10 %, le hasard fait 25 %).
  - `transition_dure_au_plus_dix_secondes` : sans `transition_done`, le gouverneur repasse en état stable à `TRANSITION_MAX_S`.
- [ ] **Step 2: Run** `cargo test -p cf-show district` — Expected: FAIL (module absent).
- [ ] **Step 3: Implement** les types et `Governor::tick`. Règles dans cet ordre de garde : show absent → `NoShow` ; frozen → `Frozen` ; transition en cours → `Transitioning` ; `Peak` → `PeakSection` ; `drop_imminent` → `DropImminent` ; fenêtre après drop (`AFTER_DROP_BEATS` mesures de 4 temps au BPM, sinon `AFTER_DROP_FALLBACK_S`) → `AfterDrop` ; moins de `MIN_DWELL_S` depuis le dernier changement → `TooSoon` ; fenêtre ouverte si `Calm`, ou `section_changed`, ou `event_ended`, ou `MAX_DWELL_S` dépassé, sinon `NoWindow`. Choix : poids par type (penchant ci-dessus) à 75 % + 25 % uniforme, exclusion du courant, puis des deux derniers si possible, variante tirée parmi celles différentes de la précédente du même type. Générateur pseudo-aléatoire local (xorshift), sans nouvelle dépendance. Transition choisie : `Gradient` par défaut (les autres variantes sont tirables seulement par `force`).
- [ ] **Step 4: Run** `cargo test -p cf-show district` — Expected: PASS ; `cargo clippy -p cf-show --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `[X-11] Gouverneur de quartiers`.

### Task 2: Plan de quartiers dans la génération de la ville (cf-world-neon)

**Files:**
- Create: `crates/cf-world-neon/src/scenery/districts.rs` (plan et mélange, pur), `crates/cf-world-neon/src/scenery/districts_tests.rs`
- Modify: `crates/cf-world-neon/src/scenery/config.rs` (champ + accesseurs), `crates/cf-world-neon/src/scenery/layout.rs` (appliquer les réglages du quartier dans `Row::generate`), `crates/cf-world-neon/src/scenery/mod.rs`

**Interfaces:**
- Consumes: `DistrictKind`, `DistrictId`, `TransitionKind` de la tâche 1 (`cf_show::district`).
- Produces :
  - `pub struct DistrictPlan` (dans `SceneryConfig`, défaut = `DistrictPlan::default()` : un seul segment `Neon` variante 0 sur toute la route) avec `fn district_at(&self, index: i64) -> DistrictId` et `fn blend_at(&self, index: i64, slot_seed: f64) -> DistrictId` (dans une zone de dégradé, choisit l'ancien ou le nouveau par hachage de `slot_seed` avec une probabilité qui monte linéairement entre `start` et `end`).
  - `SceneryConfig::push_district(&mut self, from_index: i64, to: DistrictId, blend_rows: u32)` ; `fn last_planned_index(&self) -> i64` ; `fn set_districts_active(&mut self, on: bool) -> bool` (faux = comportement historique) ; ne remplace jamais un segment déjà passé.
  - `pub struct DistrictLook { pub height_scale: f64, pub skip_chance: f64, pub temper_weights: [f64; 8] }` et `pub fn district_look(kind: DistrictKind) -> DistrictLook` ; `Neon` renvoie des valeurs neutres (`1.0`, `0.0`, poids du tirage actuel) pour que le golden ne bouge pas.
  - Marge : `pub const LEAD_ROWS: i64 = 2` ; `push_district` refuse (renvoie `false`) un `from_index` plus proche que `LEAD_ROWS` de la dernière rangée générée connue.

- [ ] **Step 1: Write the failing tests** (`districts_tests.rs`, plus un ajout au test golden existant) :
  - `golden_inchange_en_neon` : avec le plan par défaut, le contenu complet de `Row::generate` pour les index du golden est identique au fichier golden.
  - `plan_par_defaut_est_neon_partout`.
  - `segments_sont_monotones` et `ne_reecrit_pas_le_passe`.
  - `melange_deterministe` : mêmes entrées, mêmes sorties ; proportion de « nouveau » croît de ~0 à ~1 sur `blend_rows` (échantillon de 10 000 emplacements, monotone à ±3 %).
  - `banlieue_et_residentiel_different_du_neon` : sur 500 rangées, hauteur moyenne `Suburb` < hauteur moyenne `Neon` (réglage d'ordre `height_scale` ≈ 0.7) et `Residential` plus basse encore (≈ 0.45) ; distribution des tempéraments différente d'au moins 10 points pour au moins un tempérament.
  - `pas_d_allocation_dans_generate` (réutiliser le motif de `tests/facade_trail_no_alloc.rs` s'il existe pour `Row::generate`, sinon vérifier que `Row` reste `Copy`/sans `Vec`).
- [ ] **Step 2: Run** `cargo test -p cf-world-neon scenery` — Expected: FAIL.
- [ ] **Step 3: Implement.** `Row::generate` appelle `config.districts().blend_at(...)` pour obtenir le quartier de chaque emplacement et applique `district_look` : mise à l'échelle des hauteurs, probabilité de sauter l'emplacement, tirage du tempérament pondéré (seulement si `kind != Neon`, pour garder le chemin historique intact). Les valeurs de `district_look` sont mon réglage initial, à vérifier à l'œil par un humain. Fichier ≤ 400 lignes.
- [ ] **Step 4: Run** `cargo test -p cf-world-neon` — Expected: PASS (golden compris) ; clippy `-p cf-world-neon`.
- [ ] **Step 5: Commit** `[X-11] Plan de quartiers dans la génération`.

### Task 3: Étalonnage de quartier sur la palette (cf-render)

**Files:**
- Create: `crates/cf-render/src/palette/grade.rs`, `crates/cf-render/src/palette/grade_tests.rs`, `assets/palette/district_grades.ron`
- Modify: `crates/cf-render/src/palette/mod.rs` (étape 5 de `drive_palette`), `crates/cf-render/src/palette/scene.rs` (méthode `graded`), `crates/cf-render/src/lib.rs` (exports)

**Interfaces:**
- Consumes: `DistrictId` (tâche 1).
- Produces :
  - `pub struct DistrictGrade { pub hue_deg: f32, pub saturation: f32, pub lightness: f32, pub accents: [(SceneColor, f32); 3] }` (modulation bornée, voir bornes ci-dessous).
  - `pub struct GradeLibrary` : `fn builtin() -> Result<Self, PaletteError>`, `fn grade(&self, id: DistrictId) -> DistrictGrade`, `fn variants(&self, kind: DistrictKind) -> u8` ; chargé depuis `assets/palette/district_grades.ron` compilé dans le binaire (`include_str!`), comme `themes.ron`.
  - `#[derive(Resource)] pub struct DistrictGradeState` avec `fn set_target(&mut self, id: DistrictId, fade_s: f32)`, `fn update(&mut self, dt_s: f32) -> DistrictGrade` (interpolation linéaire entre étalonnages, jamais d'à-coup) ; `ScenePalette::graded(&self, grade: &DistrictGrade) -> ScenePalette`.
  - Bornes : `|hue_deg| ≤ 30`, `saturation ∈ [0.8, 1.2]`, `lightness ∈ [0.9, 1.1]`, poids d'accent ∈ [0.8, 1.25] ; `DistrictGrade::neutral()` (identité) pour `Neon` variante 0 : l'étalonnage neutre rend la palette inchangée à l'octet près.

- [ ] **Step 1: Write the failing tests** :
  - `neutre_ne_change_rien` : `graded(&neutral)` égal à l'entrée octet pour octet.
  - `bornes_respectees` : toutes les variantes livrées respectent les bornes ci-dessus.
  - `plus_petit_qu_un_changement_de_theme` : pour chaque thème du show et chaque étalonnage, l'écart moyen (distance de teinte/luminosité par rôle) entre `palette` et `palette.graded(g)` est strictement inférieur à l'écart minimal entre deux thèmes de `themes.ron`.
  - `luminance_voisine` : pour chaque variante d'un même type, la luminance moyenne varie de moins de 10 % ; et le saut de luminance moyen entre deux étalonnages (fondu) reste sous le seuil toléré (≤ 0,10 d'écart relatif, à reporter dans un commentaire avec la règle 7).
  - `fondu_continu` : `DistrictGradeState::update` ne produit jamais un pas de valeur plus grand que `(écart total) × dt / fade_s × 1,01`.
  - `theme_du_show_inchange_par_changement_de_dj` : changer de thème en plein étalonnage ne remet pas l'étalonnage à zéro (test d'intégration Bevy dans `crates/cf-render/tests/palette.rs`, motif des tests existants).
  - `ron_livre_valide` : le RON se lit, chaque type a 6 à 8 variantes.
- [ ] **Step 2: Run** `cargo test -p cf-render grade` — Expected: FAIL.
- [ ] **Step 3: Implement.** Dans `drive_palette`, l'étape 5 devient : fondu, rotation de teinte du look, **puis** `graded(state.update(dt))`. Les étalonnages RON sont écrits à la main : `Neon` plus froid et plus saturé, `Suburb` plus chaud (ambre et rose en avant), `Residential` plus doux et un peu plus froid (glace et mint en avant), avec des variantes de teinte régulièrement espacées. Valeurs initiales à valider à l'œil. Le démarrage de l'étalonnage attend le temps fort suivant (réutiliser `ThemeFader::request_on_downbeat` ou une file équivalente) avec un fondu de 10 s maximum.
- [ ] **Step 4: Run** `cargo test -p cf-render` — Expected: PASS ; clippy `-p cf-render`.
- [ ] **Step 5: Commit** `[X-11] Étalonnage de quartier`.

### Task 4: Colle Bevy : état, tick du gouverneur, événements (cf-world-neon, cf-render)

**Files:**
- Create: `crates/cf-world-neon/src/districts/mod.rs` (plugin), `crates/cf-world-neon/src/districts/state.rs`, `crates/cf-world-neon/src/districts/systems.rs`, `crates/cf-world-neon/tests/districts.rs`
- Modify: `crates/cf-world-neon/src/lib.rs` (ajouter le plugin), `crates/cf-world-neon/src/world.rs` (relayer le plan à `SceneryConfig`), `crates/cf-render/src/lib.rs` (exporter `DistrictState`, `DistrictOrders` si la fenêtre opérateur en a besoin)

**Interfaces:**
- Consumes: `Governor`, `GovernorSnapshot`, `Decision`, `TransitionKind` (tâche 1) ; `SceneryConfig::push_district` (tâche 2) ; `DistrictGradeState`, `GradeLibrary` (tâche 3) ; `AudioFeatures`, `CurrentShow`, `ShowRuntime.director.drop_plan()`, `TrafficDirector::active_names()`, `WorldEventCatalog`.
- Produces (utilisés par la tâche 5) : 
  - `#[derive(Resource)] pub struct DistrictState { pub current: DistrictId, pub target: Option<DistrictId>, pub transition: TransitionKind, pub progress: f32, pub last_reason: StayReason, pub frozen: bool, pub history: [Option<DistrictId>; 5], pub fallback: bool }` (copie lisible par l'interface, pas de logique).
  - `pub enum DistrictOrder { Force { to: DistrictId, transition: TransitionKind }, SetFrozen(bool) }` envoyé en `Message` (`app.add_message::<DistrictOrder>()`) ; l'interface n'écrit que ces ordres.
  - Familles de catalogue : un `WorldEvent { id: "district:neon" | "district:suburb" | "district:residential", family: "Quartiers", .. }` par type (variante tirée par le gouverneur) ; le déclenchement par `Action::Trigger` est relayé en `DistrictOrder::Force` (même chemin MIDI que les autres événements).

- [ ] **Step 1: Write the failing tests** (`tests/districts.rs`, `App` Bevy minimal comme `tests/palette.rs`) :
  - `peak_ne_declenche_pas` : instantané `Peak` pendant 20 min, `DistrictState.current` ne change jamais.
  - `calme_declenche_apres_le_temps_minimum` : après 150 s de Calm, `target` apparaît, le plan de `SceneryConfig` reçoit un segment à au moins `LEAD_ROWS` rangées devant la dernière rangée générée, et `current` devient `target` à la fin de la transition.
  - `fin_de_transition_au_plus_10s` : `progress` atteint 1.0 en ≤ 10 s de temps simulé et la transition est marquée finie.
  - `evenement_termine_ouvre_une_fenetre` : `TrafficDirector` perd un événement actif → `event_ended` vrai une image, sous les mêmes interdits.
  - `trigger_district_force` : `Action::Trigger("district:suburb")` donne `Force` même en `Peak` (outil debug) et affiche `fallback = false` pour `Gradient`.
  - `transition_non_implementee_retombe_sur_le_degrade` : `Force { transition: Bridge }` donne `fallback = true` et applique un dégradé.
  - `reproductible` : même graine, mêmes instantanés, même `history`.
  - `pas_d_alloc_en_regime_etabli` (motif des tests `*_no_alloc.rs` existants) pour le système de tick.
- [ ] **Step 2: Run** `cargo test -p cf-world-neon --test districts` — Expected: FAIL.
- [ ] **Step 3: Implement** le plugin : un système `tick_districts` (après l'analyse audio, avant `plan_chunk`) construit le `GovernorSnapshot` à partir de `AudioFeatures`, `CurrentShow.0.drop_now`, `drop_plan().is_some()`, `show_present`, et un `event_ended` calculé en comparant `active_count()` d'une image à l'autre ; applique la `Decision` (pousse un segment dans `SceneryConfig` avec `blend_rows` calculé à partir de la vitesse courante et de 10 s, en respectant `LEAD_ROWS`, demande `replan` seulement si le plan change pour des rangées déjà vivantes mais futures ; appelle `DistrictGradeState::set_target`). La graine du gouverneur vient de la graine du monde. Le gouverneur est dans une ressource `DistrictBrain(Governor)`. Les segments déjà passés sont retirés quand ils sont hors de la fenêtre de streaming.
- [ ] **Step 4: Run** `cargo test -p cf-world-neon` — Expected: PASS ; clippy `-p cf-world-neon`.
- [ ] **Step 5: Commit** `[X-11] Quartiers : état, gouverneur et événements`.

### Task 5: Panneau F4 refait (cf-operator)

**Files:**
- Create: `crates/cf-operator/src/debug_panel/mod.rs` (fenêtre, `DebugState`, `ScrollArea`), `crates/cf-operator/src/debug_panel/events.rs` (existant déplacé : `plan`, `fire`, `draw_events`), `crates/cf-operator/src/debug_panel/districts.rs` (nouvelle section), `crates/cf-operator/src/debug_panel/persist.rs` (structure typée), `crates/cf-operator/src/debug_panel/tests.rs`
- Modify: suppression de `crates/cf-operator/src/debug_panel.rs` (remplacé par le dossier), `crates/cf-operator/src/lib.rs` si les chemins de module changent, `docs/guide-operateur.md`

**Interfaces:**
- Consumes: `DistrictState`, `DistrictOrder` (tâche 4), `DistrictKind`, `DistrictId`, `TransitionKind`, `StayReason` (tâche 1), `GradeLibrary::variants` (tâche 3).
- Produces : `struct DebugPanelPrefs { open_sections: BTreeSet<String>, ... }` (typée), lue/écrite via `AppSettings` ; les clés historiques `debug-open:*` et `tuning:*` de `collapsed_panels` continuent d'être comprises à la lecture (migration silencieuse) et le panneau n'écrit plus dedans les nouvelles préférences.

- [ ] **Step 1: Write the failing tests** (`tests.rs`) :
  - Reprendre les tests existants de `plan`, `fire` (8 ordres par image), `choose_car` et les familles ouvertes tels quels (ils doivent passer sans changement de comportement).
  - `section_quartiers_envoie_les_bons_ordres` : « Forcer » avec type, variante et transition produit exactement un `DistrictOrder::Force` ; l'interrupteur « Geler » produit `SetFrozen(true/false)`.
  - `rien_hors_preparation` : en `OperatorMode::Live`, aucune section et aucun ordre.
  - `repli_affiche` : choisir `Bridge`/`Fog`/… affiche « (repli : dégradé) » ; `Gradient` ne l'affiche pas.
  - `etat_affiche_raison` : `DistrictState.last_reason = DropImminent` donne le libellé « bloqué : drop imminent » (et un libellé pour chaque `StayReason`).
  - `migration_des_cles` : un `collapsed_panels` contenant `debug-open:Trafic` et `tuning:car-follow:50` donne les mêmes sections ouvertes et le même curseur qu'avant.
  - `gel_et_forcage_non_sauvegardes` : après un tour de sauvegarde/rechargement, `frozen` et la cible forcée sont absents.
- [ ] **Step 2: Run** `cargo test -p cf-operator debug_panel` — Expected: FAIL.
- [ ] **Step 3: Implement.** Une `ScrollArea::vertical` globale ; sections `CollapsingHeader` : Façades (curseurs actuels), Voiture, **Quartiers** (quartier courant et variante, barre de progression de la transition, raison de la dernière décision, ComboBox type, DragValue/ComboBox variante, ComboBox transition avec 7 entrées, curseur de durée 1 à 10 s, bouton « Forcer », case « Geler l'automatique », liste des 5 derniers), Événements (liste par famille, recherche). Chaque fichier ≤ 400 lignes. Mettre à jour `docs/guide-operateur.md` (section Quartiers du panneau F4).
- [ ] **Step 4: Run** `cargo test -p cf-operator` — Expected: PASS ; clippy `-p cf-operator`.
- [ ] **Step 5: Commit** `[X-11] Panneau Debug monde refait avec les quartiers`.

### Task 6: Repli qualité basse, vérifications d'ensemble et notes de livraison

**Files:**
- Modify: `crates/cf-world-neon/src/districts/systems.rs`, `crates/cf-world-neon/src/scenery/config.rs` (réutiliser `set_quality`/`LOW_QUALITY_LEVEL` si utile), `docs/guide-operateur.md`
- Create: `crates/cf-world-neon/tests/districts_golden.rs` (garde-fou du golden à l'échelle de l'app), `docs/superpowers/plans/…` inchangé

**Interfaces:**
- Consumes: tout ce qui précède, `QualityLevers`/`QualityMode` (existants).

- [ ] **Step 1: Write the failing tests** :
  - `qualite_basse_coupe_l_etalonnage_progressivement` : en qualité basse, `DistrictGradeState` interpole vers l'étalonnage neutre en 1,5 s (rampe linéaire, motif de `LowQualityMix`), jamais d'à-coup ; les quartiers continuent de changer (la géométrie ne coûte pas plus cher).
  - `golden_app_inchange` : lancer l'app sans show (donc sans gouverneur actif) et vérifier que le contenu généré est identique au golden.
  - `lancement_compile_les_shaders` : lancer l'app 30 s et vérifier l'absence d'erreur de pipeline autre que l'erreur déjà connue `DejaVuSans-Bold.ttf` (même procédure que pour X-09).
- [ ] **Step 2: Run** les trois tests — Expected: FAIL.
- [ ] **Step 3: Implement** le repli, ajuster le guide opérateur (limites : aucune vérification visuelle faite, valeurs d'étalonnage à régler à l'œil).
- [ ] **Step 4: Run** `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace` — Expected: tout vert.
- [ ] **Step 5: Commit** `[X-11] Repli qualité et vérifications`. Dans la description de PR : cocher les critères d'acceptation avec preuves, et dans « Écarts » lister ce qui n'a pas pu être vérifié (rendu à l'écran, temps d'image avant/après, captures `docs/captures/X-11/`).
