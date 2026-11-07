# Interface opérateur à onglets : plan d'implémentation

> **Pour les agents :** sous-skill requis : `superpowers:subagent-driven-development` (recommandé) ou `superpowers:executing-plans`. Les étapes utilisent des cases `- [ ]`.

**Objectif :** remplacer la fenêtre opérateur monolithique par une coque (barre de statut + onglets Réglages / Statistiques / Debug), avec journal copiable, événements actifs par catégorie, mode manuel, DJ persistants, choix de l'écran de rendu et icône d'application.

**Architecture :** `cf-operator` est réorganisé en `shell/` + `tabs/` + `services/`. Les panneaux existants sont déplacés, pas réécrits. Les types sans Bevy (tampon de journal, réglages) vivent dans `cf-debuglog` et `cf-show` ; les ressources partagées avec le monde (`WorldAutomation`, `ActiveEvents`) dans `cf-render`, car `cf-operator` ne dépend que de `cf-render`.

**Stack :** Rust 2024, Bevy 0.20, bevy_egui, serde + RON, tracing. Tests : `cargo nextest`.

**Spec :** `docs/superpowers/specs/2026-11-07-interface-operateur-design.md`. Maquette : `docs/mockups/operator-ui-poc.html`.

**Travail sur `main` :** l'utilisateur autorise les commits et pushs directs sur `main` pour ce chantier. Un commit par tâche.

## Contraintes globales

Reprises de `CLAUDE.md` du dépôt, valables pour chaque tâche :
- Pas de `unwrap()` / `expect()` / `panic!` hors tests ; erreurs en `thiserror` (bibliothèques).
- Aucun log, verrou ni allocation dans les callbacks audio `cpal` : le tampon de journal n'est alimenté que depuis le recorder.
- Les crates `cf-audio`, `cf-analysis`, `cf-control`, `cf-show`, `cf-debuglog` ne dépendent pas de Bevy.
- Fichier de plus de 400 lignes (hors tests) : à découper.
- Commentaires et docs en français, identifiants en anglais.
- Tout flash passe par le limiteur de `cf-show` : le mode manuel ne le contourne jamais.
- Avant chaque commit : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace`. Itérer avec `-p <crate>`, lancer le workspace une fois par tâche. Compiler avec `CARGO_INCREMENTAL=0`.
- Bloqué deux fois sur le même critère : s'arrêter et le noter dans le message de commit.

## Revue : cas que les tâches doivent couvrir

- Un `settings.ron` de version 1, tronqué, ou avec un champ invalide ne doit jamais empêcher de démarrer (task 2).
- L'écran de sortie mémorisé n'est plus branché : retomber sur l'écran principal **sans effacer** le réglage (task 9).
- Une famille d'événements du catalogue absente de la table des colonnes : test en échec, pas d'oubli silencieux (task 6).
- Un directeur du monde qui ignorerait le mode manuel (task 4).
- Un événement refusé (limiteur, file pleine) ne doit pas apparaître « en cours » (task 3).
- Copier le journal quand il est vide, quand le filtre ne laisse rien, ou quand N dépasse le nombre de lignes (task 1).
- Changer de DJ avec une fiche modifiée, ou fermer l'appli avec des modifications non enregistrées (task 8).

---

### Task 1: tampon de journal et formatage pour agent

**Fichiers :**
- Créer : `crates/cf-debuglog/src/buffer.rs`
- Modifier : `crates/cf-debuglog/src/lib.rs` (export), `crates/cf-debuglog/src/recorder.rs` (`Recorder::point` pousse dans le tampon), `crates/cf-debuglog/src/layer.rs` si le chemin `tracing` n'est pas couvert par `Recorder::point`
- Test : `crates/cf-debuglog/src/buffer.rs` (module `tests`)

**Interfaces :**
- Produit (utilisé par les tâches 7 et 11) :
  - `pub struct LogLine { pub seq: u64, pub uptime_ms: u64, pub frame: u64, pub level: Level, pub ev: String, pub label: String }` (`Clone`, `Debug`).
  - `pub struct LogBuffer` : `Clone` (poignée partagée), `LogBuffer::new(capacity: usize) -> Self`, `push(&self, line: LogLine)`, `snapshot(&self) -> Vec<LogLine>` (du plus ancien au plus récent), `clear(&self)`, `set_capacity(&self, capacity: usize)`.
  - `pub struct LogFilter { pub levels: BTreeSet<Level>, pub text: String }` ; `LogFilter::matches(&self, line: &LogLine) -> bool` (texte insensible à la casse, cherché dans `ev` et `label`).
  - `pub fn format_for_agent(lines: &[LogLine], filter: &LogFilter, last: usize, header: &SessionHeader) -> String`, avec `SessionHeader { session: String, copied_at: String, context: Vec<(String, String)> }` (le contexte complet de la task 11 y passe ses paires clé/valeur).
  - `Recorder::log_buffer(&self) -> LogBuffer`.
- Consomme : `Level` de `recorder.rs`.

- [ ] **Étape 1 : tests en échec** dans `buffer.rs` : `ring_keeps_last_capacity_in_order` (capacité 3, 5 pushes, on lit les n°3,4,5), `set_capacity_shrinks_from_the_oldest`, `filter_matches_level_and_text_case_insensitive`, `format_takes_last_n_after_filter` (filtre appliqué **avant** de couper aux N dernières), `format_with_empty_buffer_has_header_and_no_lines`, `format_with_n_larger_than_len_returns_all`, `format_header_lists_levels_and_filter` (l'en-tête reprend session, date, niveaux, filtre, format `#seq +uptime frame NIVEAU événement détail`).
- [ ] **Étape 2 :** `cargo nextest run -p cf-debuglog buffer` : doit échouer (module absent).
- [ ] **Étape 3 :** implémenter `LogBuffer` (poignée `Arc<Mutex<VecDeque<LogLine>>>`, un verrou empoisonné se récupère avec `into_inner` plutôt que de paniquer) et `format_for_agent`. Une ligne se formate `#{seq} +{uptime_s:.3}s f{frame} {LEVEL:<5} {ev} {label}`.
- [ ] **Étape 4 :** brancher `Recorder::point` (et donc la couche `tracing`) sur le tampon, capacité par défaut 500. Test `recorder_point_lands_in_buffer` : un `point` apparaît dans `log_buffer().snapshot()` avec le bon niveau.
- [ ] **Étape 5 :** `cargo nextest run -p cf-debuglog` : tout passe. `cargo clippy -p cf-debuglog --all-targets -- -D warnings`.
- [ ] **Étape 6 : commit** `Journal : tampon circulaire et copie pour agent`, puis `git push origin main`.

---

### Task 2: réglages version 2 (préférences de l'opérateur)

**Fichiers :**
- Modifier : `crates/cf-show/src/persist/settings.rs`
- Test : même fichier, module de tests existant

**Interfaces :**
- Produit : `pub struct OperatorPrefs` (`Serialize`, `Deserialize`, `Default`, `#[serde(default)]`, champs `lenient`) :
  - `last_tab: String` (`"settings" | "stats" | "debug"`, défaut `"settings"`),
  - `pinned_events: BTreeSet<String>` (identifiants `WorldEvent::id`),
  - `output_monitor: Option<String>` (nom de l'écran),
  - `fullscreen: bool` (défaut `true`),
  - `active_profile: Option<String>`,
  - `last_event_path: Option<String>` (dernier événement ouvert ou enregistré par l'opérateur).
  - `AppSettings.operator: OperatorPrefs` ; `SETTINGS_VERSION = 2`.

- [ ] **Étape 1 : tests en échec** : `v1_file_still_loads_with_default_operator_prefs` (un RON de version 1 existant, copié d'un test présent, se lit et `operator == default`), `operator_prefs_roundtrip`, `invalid_operator_field_falls_back_to_default_without_losing_others` (un `pinned_events` de mauvais type ne fait pas perdre `car`).
- [ ] **Étape 2 :** `cargo nextest run -p cf-show settings` : échec.
- [ ] **Étape 3 :** ajouter la structure et le champ, passer la version à 2. Pas d'autre migration : le défaut suffit.
- [ ] **Étape 4 :** `cargo nextest run -p cf-show` et clippy sur `cf-show`.
- [ ] **Étape 5 : commit** `Réglages v2 : préférences de l'opérateur`, push.

---

### Task 3: `WorldAutomation` et `ActiveEvents` (cf-render)

**Fichiers :**
- Créer : `crates/cf-render/src/automation.rs`, `crates/cf-render/src/active_events.rs`
- Modifier : `crates/cf-render/src/lib.rs` (modules + réexport)
- Test : dans chaque fichier

**Interfaces :**
- Produit (utilisé par les tâches 4, 6) :
  - `#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)] pub struct WorldAutomation { pub manual: bool }` ; `WorldAutomation::allows_spontaneous(self) -> bool` (`!manual`).
  - `pub enum EventCategory { Sign, Building, Car, District }` (`Copy`, `Eq`, `Hash`, `Debug`), `EventCategory::ALL`.
  - `pub enum EventOrigin { Manual, Auto }`.
  - `pub struct ActiveEvent { pub id: ActiveId, pub event_id: &'static str, pub label: String, pub category: EventCategory, pub origin: EventOrigin, pub started_s: f64, pub duration_s: Option<f64> }`.
  - `#[derive(Resource, Default)] pub struct ActiveEvents` : `begin(&mut self, event: ActiveEvent) -> ActiveId`, `end(&mut self, id: ActiveId)`, `end_category(&mut self, c: EventCategory)`, `clear(&mut self)`, `iter(&self) -> impl Iterator<Item=&ActiveEvent>`, `in_category(&self, c) -> impl Iterator`, `progress(&self, id, now_s: f64) -> Option<f32>` (`None` si la durée est inconnue ; sinon borné à 0..=1), `len(&self) -> usize`, `ActiveEvents::expire(&mut self, now_s: f64)` retire les événements à durée connue dépassée.
  - `ActiveId(u64)`, `Copy`, `Eq`.

- [ ] **Étape 1 : tests en échec** : `progress_is_clamped_between_zero_and_one`, `progress_is_none_without_duration`, `expire_removes_only_finished_events_with_known_duration`, `end_category_keeps_other_categories`, `begin_returns_distinct_ids`, `default_automation_allows_spontaneous`.
- [ ] **Étape 2 :** `cargo nextest run -p cf-render automation active_events` : échec.
- [ ] **Étape 3 :** implémenter. Le registre est une `Vec` parcourue linéairement (quelques dizaines d'entrées au plus). Ajouter un système `expire_active_events` (planifié par un petit `ActiveEventsPlugin`, ajouté par `CfRenderPlugin` ou son équivalent) qui appelle `expire` avec l'horloge de simulation (`SimClock`, qui s'arrête en pause).
- [ ] **Étape 4 :** vérifier `cargo nextest run -p cf-render` et clippy.
- [ ] **Étape 5 : commit** `cf-render : mode manuel du monde et événements actifs`, push.

---

### Task 4: les directeurs du monde respectent le mode manuel et publient leurs événements

**Fichiers :**
- Modifier : les directeurs de `crates/cf-world-neon/src/` : `stunts/director.rs`, `traffic/director.rs`, `djsign/director.rs`, `signs/` (publication des enseignes/pubs), `traffic/grand/landmark.rs`, `districts/systems.rs` (changements de quartier automatiques). L'implémenteur dresse la liste exacte en cherchant tout ce qui lance un événement de lui-même (`grep -rn "Orchestr\|orchestr"` donne le point de départ) et l'ajoute en commentaire du test.
- Créer : `crates/cf-world-neon/src/automation_tests.rs`

**Interfaces :**
- Consomme : `WorldAutomation`, `ActiveEvents`, `ActiveEvent`, `EventCategory`, `EventOrigin` (task 3).
- Règle : chaque directeur lit `Res<WorldAutomation>` (absent = autorisé) et ne lance **rien de spontané** quand `manual` est vrai. Un déclenchement venant d'une `Action::Trigger` passe toujours. Le directeur écrit dans `ActiveEvents` **au moment où le monde accepte le lancement**, avec `EventOrigin::Auto` ou `Manual` selon la source, et l'enlève à la fin de l'événement. Un lancement refusé (limiteur, pas de place) n'écrit rien.
- Rangement catégorie : enseignes, pubs, ticker, graffitis, projecteurs holographiques → `Sign` ; immeubles-vitrines, façades, repères → `Building` ; trafic, cascades, motos → `Car` ; changements de quartier → `District`.

- [ ] **Étape 1 : tests en échec** dans `automation_tests.rs` (application Bevy minimale, une par directeur de la liste) : `<directeur>_spawns_nothing_spontaneously_in_manual_mode` (on fait avancer le temps assez longtemps pour que l'automatique aurait lancé, et `ActiveEvents` reste vide), `<directeur>_still_obeys_explicit_trigger_in_manual_mode`, `<directeur>_publishes_and_clears_active_event`, `refused_trigger_is_not_listed_as_active`. Un test de garde `every_known_director_is_covered` compare la liste des directeurs testés à une constante déclarée à côté du code des directeurs, pour qu'en ajouter un sans test casse ce test.
- [ ] **Étape 2 :** `cargo nextest run -p cf-world-neon automation` : échec.
- [ ] **Étape 3 :** brancher chaque directeur (un petit helper `fn may_spawn(automation: Option<&WorldAutomation>) -> bool` dans `cf-world-neon`, utilisé partout). Commit intermédiaire possible par directeur.
- [ ] **Étape 4 :** `cargo nextest run -p cf-world-neon`, clippy. Aucun test existant ne change de comportement.
- [ ] **Étape 5 : commit** `Monde : mode manuel et suivi des événements actifs`, push.

---

### Task 5: coque, onglets et barre de statut (panneaux déplacés sans changement)

**Fichiers :**
- Créer : `crates/cf-operator/src/shell/mod.rs` (état, enum des onglets), `shell/status_bar.rs`, `shell/shortcuts.rs`, `shell/help.rs`, `crates/cf-operator/src/tabs/mod.rs`, `tabs/settings/mod.rs`, `tabs/stats/mod.rs`, `tabs/debug/mod.rs`
- Modifier : `crates/cf-operator/src/lib.rs` (modules, systèmes), `ui.rs` (`main_panel` délègue à la coque), `sim_bar.rs` (son contenu devient une partie de `status_bar`)
- Test : `shell/tests.rs`

**Interfaces :**
- Produit :
  - `#[derive(Resource)] pub(crate) struct ShellState { pub tab: Tab, pub help_open: bool, pub confirm_next_dj: bool, pub confirm_reset: bool }` ; `Tab { Settings, Stats, Debug }` avec `Tab::key() -> &'static str` (les valeurs de `OperatorPrefs::last_tab`) et `Tab::visible_in(mode: OperatorMode) -> bool` (`Debug` faux en Live).
  - `fn tab_for_shortcut(key: egui::Key, mode: OperatorMode) -> Option<Tab>` (`1`, `2`, `3`).
  - `status_bar::draw(ui, &StatusData, &mut ShellState) -> Vec<StatusAction>` avec `StatusAction { Blackout, NextDj, TogglePause, ToggleManual, ConfirmReset }` ; `StatusData` regroupe les valeurs lues (source audio, fps, BPM, DJ en cours, temps restant, nombre d'événements actifs, mode manuel, pause).
- Consomme : `OperatorMode`, `SimControl`, `ActiveEvents`, `WorldAutomation`, `OperatorPrefs` (task 2).
- Comportement : la barre de statut est dessinée dans **tous** les onglets et en Live. En Live l'onglet Debug n'est ni affiché ni accessible par raccourci. « Passer au suivant » demande une confirmation en deux temps (comme le reset). Le bouton Noir est actif partout et répond à Échap. L'onglet courant est restauré au démarrage depuis `OperatorPrefs::last_tab` et sauvegardé à chaque changement.
- Les panneaux actuels sont placés dans l'onglet qui leur correspond sans modifier leur code : audio + learn + contrôles + mise à jour → Réglages ; diagnostics → Statistiques ; `debug_panel` reste provisoirement une fenêtre dans l'onglet Debug (remplacé à la task 6).

- [ ] **Étape 1 : tests en échec** : `tab_shortcuts_map_1_2_3`, `debug_tab_hidden_in_live`, `debug_shortcut_ignored_in_live`, `tab_key_roundtrips`, `restores_last_tab_and_falls_back_to_settings_on_unknown_value`, `next_dj_needs_two_clicks`, `blackout_is_always_available`.
- [ ] **Étape 2 :** `cargo nextest run -p cf-operator shell` : échec.
- [ ] **Étape 3 :** écrire la coque. `main_panel` appelle `shell::draw` ; la vue Live existante reste intacte et reçoit la barre de statut au-dessus. Le code déplacé ne change que de chemin de module : le décrire ainsi dans le commit.
- [ ] **Étape 4 :** l'aide `?` (`help.rs`) liste les raccourcis depuis une table unique `SHORTCUTS: &[(&str, &str)]` utilisée aussi pour les infobulles ; test `help_lists_every_registered_shortcut`.
- [ ] **Étape 5 :** `cargo nextest run -p cf-operator`, clippy, lancer l'appli (`cargo run -p cyber-funk`) et vérifier à l'œil que les trois onglets s'ouvrent et que rien n'a changé dans leur contenu.
- [ ] **Étape 6 : commit** `Opérateur : coque à onglets et barre de statut`, push.

---

### Task 6: onglet Debug en grille

**Fichiers :**
- Créer : `tabs/debug/columns.rs` (table familles → colonnes), `tabs/debug/grid.rs`, `tabs/debug/chip.rs`, `tabs/debug/lane.rs`, `tabs/debug/districts.rs` (repris de `debug_panel/districts.rs`), `tabs/debug/dock.rs` (plans caméra), `tabs/debug/pad.rs` (pilotage voiture), `tabs/debug/top_bar.rs`
- Modifier : `tabs/debug/mod.rs`, suppression progressive de `debug_panel/` une fois son contenu repris (façades et choix de voiture vont dans le dock ou une section repliée « Réglages monde »)
- Test : `tabs/debug/tests.rs`

**Interfaces :**
- Consomme : `WorldEventCatalog`, `WorldEvent { id, label, family, note }`, `ActiveEvents`, `EventCategory`, `WorldAutomation`, `DistrictOrder`, `DistrictState`, `Action::Trigger(TriggerId)`, `MAX_ORDERS_PER_FRAME` (repris de `debug_panel/events.rs`), `TransitionKind::{label, implemented}` de `cf_show::district`.
- Produit :
  - `fn column_for_family(family: &str) -> Option<EventCategory>` ; `fn plan_columns(catalog: &WorldEventCatalog, filter: &str, pinned: &BTreeSet<String>) -> [Vec<&WorldEvent>; 4]` (épinglés en tête, ordre du catalogue ensuite ; une famille inconnue va dans une liste « Autres » renvoyée à part).
  - `fn fire(orders: &mut Vec<Action>, event: &WorldEvent) -> bool` (reprise de la logique existante, plafond par image conservé).
  - Dock : liste des plans caméra et pad voiture écrivent des `Action`/ordres existants. **Avant d'écrire le dock**, l'implémenteur lit `cf-cinema/src/shots/mod.rs` et `cf-world-neon/src/pilot/input.rs` pour savoir ce qui est déjà pilotable. Si un plan ou une manœuvre n'a pas de point d'entrée, l'afficher grisé avec l'infobulle « pas encore pilotable » et le noter dans le message de commit (le spec exclut d'en créer).
- Mise en page : 4 colonnes égales ; dans chaque colonne, hauteur **2/3 boutons, 1/3 « En cours »**, chacune avec son propre défilement ; la ligne de la grille est forcée à la hauteur disponible. Puces : remplissage selon `progress`, `×N` si plusieurs instances, point jaune pour les événements à flash. Clic droit épingle (écrit dans `OperatorPrefs::pinned_events`). Recherche `/`. Mode manuel écrit `WorldAutomation`. « Tout arrêter » vide `ActiveEvents`.

- [ ] **Étape 1 : tests en échec** : `every_catalog_family_has_a_column` (parcourt le catalogue réel assemblé avec tous les plugins du monde ; échoue en nommant la famille manquante), `plan_columns_puts_pinned_first`, `plan_columns_filter_is_case_insensitive`, `unknown_family_goes_to_others`, `fire_respects_per_frame_cap`, `district_fallback_transition_is_marked_unimplemented`, `manual_toggle_writes_world_automation`, `stop_all_clears_active_events`, `chip_progress_uses_max_of_running_instances`.
- [ ] **Étape 2 :** `cargo nextest run -p cf-operator tabs::debug` : échec.
- [ ] **Étape 3 :** implémenter table, plan, puces, couloirs, barre du haut, dock. L'onglet n'existe qu'en Préparation (voir task 5).
- [ ] **Étape 4 :** supprimer de `debug_panel/` ce qui est repris ; garder les fonctions de persistance (`DebugPanelPrefs`) tant que `AppSettings` les contient.
- [ ] **Étape 5 :** vérification manuelle : lancer l'appli, lancer un événement par colonne, voir la puce se remplir et l'entrée « En cours » apparaître et disparaître ; basculer en manuel et constater qu'aucun événement automatique ne part pendant 30 s. Consigner le résultat dans le message de commit.
- [ ] **Étape 6 :** tests, clippy, **commit** `Opérateur : onglet Debug en grille`, push.

---

### Task 7: onglet Statistiques et journal en direct

**Fichiers :**
- Créer : `tabs/stats/tiles.rs`, `tabs/stats/charts.rs`, `tabs/stats/log_view.rs`, `tabs/stats/copy.rs`, `services/logbuf.rs` (ressource Bevy qui détient la poignée `LogBuffer`)
- Modifier : `tabs/stats/mod.rs`, `lib.rs` (insère la ressource), `crates/cyber-funk/src/logging.rs` (passe `Recorder::log_buffer()` à l'opérateur)
- Test : `tabs/stats/tests.rs`

**Interfaces :**
- Consomme : `LogBuffer`, `LogFilter`, `LogLine`, `format_for_agent`, `SessionHeader` (task 1), `DiagnosticsSnapshot`, `AudioFeatures`.
- Produit : `#[derive(Resource)] pub struct OperatorLog(pub LogBuffer)` ; `LogViewState { levels: BTreeSet<Level>, text: String, paused: bool, capacity: usize, copy_n: usize, frozen: Option<Vec<LogLine>> }` (en pause, l'affichage lit `frozen`) ; `fn copy_text(state: &LogViewState, lines: &[LogLine], header: &SessionHeader) -> String` ; `fn mark_moment(buffer: &LogBuffer, label: &str)` (pousse une ligne `ev = "mark"`, niveau info).
- Comportement : capacité 100 / 200 / 500 ; boutons de niveau ; filtre texte ; pause ; vider ; clic sur un nom d'événement pose le filtre dessus ; copier N dernières lignes **visibles** (20/50/100/200) ; variante « warn + erreurs » ; raccourci `M` pour marquer. Copie via `ctx.copy_text`. Défilement collé en bas tant que l'utilisateur n'a pas remonté.
- Disponible en Live (ne lance rien).

- [ ] **Étape 1 : tests en échec** : `copy_text_applies_levels_and_filter_then_takes_last_n`, `copy_text_on_empty_filter_result_has_header_only`, `warn_error_variant_ignores_info_even_if_visible`, `mark_moment_appears_in_snapshot_with_mark_event`, `pause_freezes_the_displayed_lines`, `lowering_capacity_drops_oldest`.
- [ ] **Étape 2 :** échec, puis implémentation (tuiles, graphes tracés avec `egui_plot` seulement s'il est déjà dans le workspace, sinon dessin direct au `Painter` ; ne pas ajouter de dépendance sans demander).
- [ ] **Étape 3 :** vérification manuelle : provoquer un avertissement, copier 50 lignes, coller, vérifier l'en-tête et l'ordre.
- [ ] **Étape 4 :** tests, clippy, **commit** `Opérateur : onglet Statistiques et journal copiable`, push.

---

### Task 8: contexte complet pour l'agent (point 7)

**Fichiers :**
- Créer : `tabs/stats/context.rs`
- Test : `tabs/stats/tests.rs`

**Interfaces :**
- Produit : `fn build_context(snapshot: &ContextSnapshot) -> Vec<(String, String)>` où `ContextSnapshot` regroupe : événements actifs (id, catégorie, pourcentage, origine), quartier et transition (`DistrictState`), plan caméra courant, DJ courant et temps restant, mode d'automatisation, pause, version de l'appli. La sortie alimente `SessionHeader.context` (task 1).
- Bouton « Copier le contexte complet » à côté de « Copier pour un agent ».

- [ ] **Étape 1 : tests en échec** : `context_lists_active_events_with_progress`, `context_marks_unknown_camera_as_none_instead_of_omitting`, `context_includes_manual_mode_and_pause`.
- [ ] **Étape 2 :** échec, implémentation, tests, clippy.
- [ ] **Étape 3 : commit** `Opérateur : copie du contexte complet`, push.

---

### Task 9: onglet Réglages (DJ, profils, import/export, reprise au relancement)

**Fichiers :**
- Créer : `tabs/settings/dj_list.rs`, `tabs/settings/dj_form.rs` (réutilise `prepare::forms` et `prepare::edit`), `tabs/settings/profiles.rs`, `tabs/settings/transfer.rs`, `tabs/settings/restore.rs`
- Modifier : `tabs/settings/mod.rs`, `prepare/session.rs` (chemin d'enregistrement par défaut, restauration), `lib.rs`
- Test : `tabs/settings/tests.rs`

**Interfaces :**
- Consomme : `PrepSession`, `prepare::edit::{add_slot, duplicate_slot, remove_slot, move_slot}`, `EventDocument::{open, save, save_as}`, `OperatorPrefs::{active_profile, last_event_path}`, `Settings`.
- **Écart au spec** (à reporter dans le spec à la fin de la tâche) : les DJ ne sont pas une liste dans `AppSettings`. Un DJ est un `DjSlot` de l'`Event`, déjà éditable et enregistrable par `PrepSession`. Un **profil de soirée** est donc un **fichier d'événement RON** dans `<config>/cyber-funk/profiles/<nom>.ron` ; `AppSettings` ne garde que le nom du profil actif et le dernier chemin ouvert. L'import/export est `EventDocument::open` / `save_as` sur un chemin choisi (extension `.ron`). Cela évite une seconde représentation des DJ.
- Produit :
  - `fn profiles_dir() -> Option<PathBuf>` ; `fn list_profiles(dir: &Path) -> Vec<String>` (tri alphabétique, ignore les fichiers illisibles) ; `fn profile_path(dir: &Path, name: &str) -> Option<PathBuf>` (refuse les noms vides, avec séparateur de chemin ou `..`).
  - `fn startup_event_path(prefs: &OperatorPrefs, profiles: &Path, cli_show: Option<&Path>) -> Option<PathBuf>` : priorité au `--show` de la ligne de commande, puis au profil actif, puis à `last_event_path`, sinon `None` (événement neuf).
  - `DjFormView` : reprend les formulaires existants d'un `DjSlot` (nom, logo, thème, horaires, profil audio, mode, cues) ; **ne pas** recréer les champs fictifs de la maquette (style, couleur libre).
- Comportement : changer de DJ avec brouillon modifié demande confirmation ; `Ctrl+S` enregistre dans le profil actif (en créant un profil « Soirée » au premier enregistrement si aucun) ; l'enregistrement est atomique (`write_atomic`) ; un fichier de profil illisible n'empêche pas de démarrer (message dans le journal, événement neuf).

- [ ] **Étape 1 : tests en échec** : `profile_path_rejects_traversal_and_empty_names`, `list_profiles_ignores_unreadable_files`, `startup_prefers_cli_then_active_profile_then_last_path`, `startup_with_nothing_returns_none`, `reorder_persists_after_save_and_reopen` (enregistre, rouvre, compare l'ordre des `slots`), `delete_last_dj_is_refused_or_leaves_valid_event` (suit la règle de `edit::remove_slot`), `corrupt_profile_falls_back_to_new_event`, `switching_dj_with_unsaved_changes_requires_confirmation`.
- [ ] **Étape 2 :** échec, puis implémentation.
- [ ] **Étape 3 :** à la fermeture (`quit`) : si le brouillon est modifié, le dialogue de sortie existant propose d'enregistrer (réutiliser `QuitConfirmation`). Test `quit_with_dirty_session_asks_to_save`.
- [ ] **Étape 4 :** vérification manuelle : créer un DJ, le renommer, réordonner, quitter, relancer sans `--show` : l'état est retrouvé.
- [ ] **Étape 5 :** tests, clippy, **commit** `Opérateur : onglet Réglages, profils et reprise`, push.

---

### Task 10: écran de sortie du rendu

**Fichiers :**
- Créer : `crates/cf-operator/src/services/screens.rs`, `tabs/settings/screens_view.rs`
- Modifier : `crates/cyber-funk/src/projector.rs` (rendre public/partagé ce qu'il faut de `resolve`, `MonitorChoice`), `crates/cyber-funk/src/main.rs` ou `wiring.rs` (lit `OperatorPrefs::output_monitor` quand `--projector-monitor` est absent)
- Test : `services/screens.rs` et `projector.rs`

**Interfaces :**
- Produit : `pub struct ScreenInfo { pub name: Option<String>, pub width: u32, pub height: u32, pub refresh_mhz: Option<u32>, pub primary: bool }` ; `fn describe_screens(monitors: &[MonitorInfo]) -> Vec<ScreenInfo>` ; `pub enum ScreenOrder { MoveProjector(String), Identify }` (message Bevy lu côté binaire, car `cf-operator` ne connaît pas le projecteur).
- Règle de priorité : option de ligne de commande > réglage mémorisé > écran principal.
- Si le nom mémorisé n'existe plus : écran principal, réglage **conservé** tel quel.
- Déplacer la fenêtre projecteur sans la recréer ; plein écran maintenu si `fullscreen`.

- [ ] **Étape 1 : tests en échec** : `cli_choice_beats_saved_choice`, `saved_missing_monitor_falls_back_to_primary_and_keeps_setting`, `saved_name_match_is_case_insensitive` (même comparaison que `resolve`), `describe_screens_marks_one_primary`.
- [ ] **Étape 2 :** échec, implémentation, tests, clippy.
- [ ] **Étape 3 :** vérification manuelle avec deux écrans si disponibles, sinon le dire dans le commit (« non vérifié sans second écran »).
- [ ] **Étape 4 : commit** `Opérateur : choix de l'écran de rendu`, push.

---

### Task 11: icône d'application

**Fichiers :**
- Créer : `tools/icons/generate.py` (ou script du même genre dans l'outillage existant de `tools/`), `assets/images/icons/` (PNG 16, 32, 48, 64, 128, 256, 512 + `app.ico`), `crates/cyber-funk/build.rs`
- Modifier : `crates/cyber-funk/Cargo.toml` (dépendance de build pour l'icône Windows ; **demander avant d'ajouter la crate**, par exemple `winresource` ou `embed-resource`), `crates/cf-operator/src/window.rs` et `crates/cyber-funk/src/projector.rs` (icône de fenêtre)
- Test : `crates/cyber-funk/src/` (test de lecture du PNG embarqué)

**Interfaces :**
- Produit : `pub fn window_icon() -> Option<bevy::window::WindowIcon>` ou équivalent selon l'API de Bevy 0.20 (vérifier avec Context7 avant d'écrire) ; les fichiers générés sont **versionnés** pour qu'un build n'exige aucun outil externe.

- [ ] **Étape 1 :** générer les PNG et l'`.ico` depuis `assets/images/logo-app.svg` (outil à installer seulement dans l'environnement du développeur, pas dans le build). Vérifier visuellement la lisibilité à 16 et 32 px ; si le logo est illisible à 16 px, le dire et proposer une variante simplifiée plutôt que de la livrer.
- [ ] **Étape 2 : test en échec** `embedded_icon_decodes_and_is_square_256` puis implémentation.
- [ ] **Étape 3 :** `build.rs` pour l'icône de l'exécutable Windows.
- [ ] **Étape 4 :** vérification manuelle : barre des tâches et titre de fenêtre. Non vérifiable en cloud : le dire.
- [ ] **Étape 5 : commit** `Icône d'application`, push.

---

### Task 12: finitions, documentation et vérification finale

**Fichiers :**
- Modifier : `docs/superpowers/specs/2026-11-07-interface-operateur-design.md` (reporter l'écart sur les profils, task 9), `CLAUDE.md` du dépôt si la structure de `cf-operator` y est décrite, skill `show-model` ou autre skill si elle cite le panneau F4
- Test : tests existants

- [ ] **Étape 1 :** parcourir le spec section par section et cocher chaque exigence face au code ; lister tout écart dans le message de commit.
- [ ] **Étape 2 :** `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace`, `cargo test --workspace --doc`. Tout doit être vert.
- [ ] **Étape 3 :** captures de chaque onglet à 1280 × 720 et 1920 × 1080 (sous `Xvfb` en cloud, sinon à la main) rangées dans `docs/captures/`.
- [ ] **Étape 4 :** vérifier qu'aucun fichier de `cf-operator` ne dépasse 400 lignes (hors tests).
- [ ] **Étape 5 : commit** `Opérateur : finitions et documentation`, push.
