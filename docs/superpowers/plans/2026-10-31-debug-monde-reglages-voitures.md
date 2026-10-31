# Réglages sauvegardés, choix de la voiture, onglet « Debug monde » — plan d'implémentation

> **Pour les agents :** sous-skill requis : `superpowers:subagent-driven-development` (recommandé) ou
> `superpowers:executing-plans`. Les étapes ont des cases `- [ ]`.

**Goal :** relancer l'application retrouve les réglages du logiciel ; l'opérateur choisit la voiture (CyberCar ou dirty
car) et lance à la main les événements du monde depuis un onglet rangé par familles.

**Architecture :** un type pur `AppSettings` (RON, lecture tolérante, écriture atomique) vit dans `cf-show::persist`,
une ressource Bevy le porte dans `cf-render` et un système l'écrit quand il change. Les deux voitures sont décodées à
l'ouverture (règle 8) et la voiture en place est reconstruite à la demande sans rien charger. L'onglet debug lit un
registre `WorldEventCatalog` que chaque famille remplit, et lance par `Action::Trigger` (le chemin du MIDI).

**Tech Stack :** Rust 2024, Bevy 0.20, bevy_egui 0.43, `ron`, `serde`, `dirs`, `tempfile` (tests).

**Spec :** conversation du 2026-10-31 (pas de document de design séparé, accord de l'utilisateur).

## Global Constraints

- Pas de `unwrap()` / `expect()` / `panic!` hors tests (règle 2).
- `cf-show` ne dépend pas de Bevy (règle 5) : `AppSettings` n'importe rien de Bevy ni de `cf-render`.
- Aucun chargement d'asset pendant un show : les deux voitures sont décodées au démarrage (règle 8).
- Tout flash lancé depuis le debug passe par le limiteur photosensible (règle 7) ; aucune porte de contournement.
- Pas d'allocation par image en régime établi dans les systèmes chauds (règle 4) : le registre d'événements est
  construit au démarrage, l'onglet debug ne s'exécute que fenêtre opérateur ouverte et hors vue Live.
- Fichiers de plus de 400 lignes à découper (règle 11). Commentaires et docs en français, identifiants en anglais.
- Avant chaque commit : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo nextest run --workspace`. Compiler avec `CARGO_INCREMENTAL=0`.
- Branche `task/X-debug-reglages` créée depuis `main`. Ne pas toucher `docs/TASKS.md`.
- Fichier de réglages : `<dirs::config_dir()>/cyber-funk/settings.ron`.

## Review Focus

- Fichier de réglages absent, vide, tronqué ou d'une version future : démarrage normal avec les défauts, jamais de
  panique ni d'erreur bloquante (test dans la tâche 1).
- Réglage qui nomme un périphérique audio ou un port MIDI absent ce jour-là : on garde la valeur écrite mais on retombe
  sur la source par défaut sans effacer le fichier (tâche 3).
- Changement de voiture en plein mouvement : la voiture garde sa position, sa vitesse mesurée et sa teinte ; aucune
  entité de l'ancienne ne reste (tâche 5).
- Clic répété sur « Lancer » : la file de déclencheurs est bornée, les clics en trop sont perdus sans bruit (tâche 6).
- Événement lancé avec un look qui interdit les flashs : le limiteur garde la main (tâche 6).
- Écriture des réglages pendant un show : pas d'écriture sur le fil d'image, pas plus d'une écriture par seconde
  (tâche 2).

---

### Task 1 : `AppSettings` pur, lecture tolérante, écriture atomique

**Files :**
- Create : `crates/cf-show/src/persist/settings.rs`
- Modify : `crates/cf-show/src/persist/mod.rs` (déclarer le module, ré-exporter `AppSettings`, `SETTINGS_VERSION`)
- Test : `crates/cf-show/tests/settings.rs`

**Interfaces :**
- Produces :
  - `pub const SETTINGS_VERSION: u32 = 1;`
  - `pub struct AppSettings { pub version: u32, pub audio_source: Option<String>, pub mic_device: Option<String>,
    pub line_device: Option<String>, pub midi_inputs: Vec<String>, pub quality: Option<String>,
    pub car: CarChoice, pub collapsed_panels: Vec<String> }` (`Serialize, Deserialize, Default, Clone, PartialEq`,
    `#[serde(default)]` sur la structure pour qu'un champ manquant prenne son défaut).
  - `pub enum CarChoice { Dirty, Cyber }` (défaut `Dirty`, `#[serde(other)]`-équivalent : une valeur inconnue donne
    le défaut).
  - `impl AppSettings { pub fn load(path: &Path) -> Self; pub fn save(&self, path: &Path) -> Result<(), PersistError>; }`
  - `load` ne renvoie jamais d'erreur : fichier absent, illisible ou invalide donne `Self::default()` et un
    `tracing::warn!`.

- [ ] **Step 1 : tests.** Dans `tests/settings.rs` : `un_fichier_absent_donne_les_defauts`,
  `un_fichier_tronque_donne_les_defauts`, `un_champ_inconnu_est_ignore_et_les_autres_sont_gardes`,
  `une_voiture_inconnue_donne_la_dirty_car`, `un_aller_retour_conserve_tout` (écrit puis relit dans un `tempfile`),
  `l_ecriture_laisse_l_ancien_fichier_intact_si_le_nouveau_est_refuse` (dossier parent en lecture seule ou chemin
  invalide : `save` renvoie `Err`, l'ancien contenu est inchangé).
- [ ] **Step 2 :** `cargo nextest run -p cf-show --test settings` : échec de compilation attendu.
- [ ] **Step 3 :** implémenter le module. `save` passe par `write_atomic` (déjà dans `persist::io`) et crée le dossier
  parent. `load` lit en `ron::from_str` ; un échec de syntaxe donne les défauts, un échec sur un champ isolé garde les
  autres champs (désérialiser champ par champ avec `#[serde(default)]` et `deserialize_with` tolérant pour `car`).
- [ ] **Step 4 :** `cargo nextest run -p cf-show` : vert.
- [ ] **Step 5 : commit** `[X-01] Réglages de l'application : type pur et fichier RON tolérant`.

### Task 2 : ressource Bevy et écriture automatique

**Files :**
- Create : `crates/cf-render/src/settings/mod.rs`, `crates/cf-render/src/settings/systems.rs`
- Modify : `crates/cf-render/src/lib.rs` (déclarer, ré-exporter `SettingsPlugin`, `Settings`)
- Test : `crates/cf-render/src/settings/tests.rs`

**Interfaces :**
- Consumes : `cf_show::persist::AppSettings`.
- Produces :
  - `#[derive(Resource)] pub struct Settings { pub value: AppSettings, path: PathBuf }` avec
    `Settings::new(value, path)`. Un `DerefMut` sur `value` déclenche la détection de changement de Bevy.
  - `pub struct SettingsPlugin { pub path: PathBuf }` : insère `Settings` (chargé par `AppSettings::load`) en
    `PreStartup` et ajoute `save_when_changed`.
  - `save_when_changed` : si `Settings` a changé, attend 1,0 s sans nouveau changement (temps réel, `Time<Real>`), puis
    écrit sur le `IoTaskPool` (jamais sur le fil d'image). Une écriture en cours n'est pas doublée ; un changement
    arrivé pendant l'écriture en programme une autre. Un échec d'écriture donne un `tracing::warn!` unique par minute.
  - Fonction pure `Debounce::step(&mut self, changed: bool, now_s: f64) -> bool` testable seule.

- [ ] **Step 1 : tests** sur `Debounce` : `pas_d_ecriture_sans_changement`, `une_rafale_donne_une_seule_ecriture`,
  `un_changement_pendant_l_ecriture_en_programme_une_autre`.
- [ ] **Step 2 :** lancer, échec attendu. **Step 3 :** implémenter. **Step 4 :** lancer, vert.
- [ ] **Step 5 : commit** `[X-02] Écriture automatique des réglages (1 s de calme, hors fil d'image)`.

### Task 3 : brancher les réglages existants

**Files :**
- Modify : `crates/cyber-funk/src/main.rs` (ajoute `SettingsPlugin`, calcule le chemin avec `dirs::config_dir()`),
  `crates/cyber-funk/src/wiring.rs` ou `show_setup.rs` (appliquer les réglages relus aux configs),
  `crates/cf-render/src/audio/` (recopier `AudioCommand::SelectSource` et `SelectDevice` dans `Settings`),
  `crates/cf-render/src/controls/` (recopier les ports MIDI actifs dans `Settings::midi_inputs`),
  `crates/cf-operator/src/ui.rs` (recopier l'état replié des panneaux dans `collapsed_panels`).
- Test : `crates/cf-render/src/settings/tests.rs`, `crates/cyber-funk/tests/` (si le dossier existe, sinon dans
  `wiring.rs`).

**Interfaces :**
- Consumes : `Settings` (tâche 2).
- Produces : `fn apply_to_audio_config(settings: &AppSettings, config: &mut AudioConfig)` dans le binaire.

- [ ] **Step 1 : test** `un_peripherique_absent_retombe_sur_la_source_par_defaut_sans_effacer_le_reglage` : la config
  audio construite depuis des réglages qui nomment un périphérique inconnu garde `Demo`, et `Settings.value` conserve
  le nom.
- [ ] **Step 2 :** implémenter. Le niveau de qualité est enregistré seulement si l'opérateur le fixe lui-même ; le mode
  adaptatif automatique de `QualityPlugin` n'écrit rien (lire `quality/mod.rs` pour trouver le point où l'opérateur le
  fixe ; s'il n'existe pas aujourd'hui, ne pas en créer et laisser `quality` à `None`, à noter dans la PR « Écarts »).
- [ ] **Step 3 :** lancer l'application deux fois : changer le périphérique audio, quitter, relancer, constater qu'il
  est repris. Noter dans la PR ce qui n'a pas pu être vérifié sans matériel.
- [ ] **Step 4 : commit** `[X-03] Les réglages audio, MIDI et panneaux survivent au relancement`.

### Task 4 : deux voitures, quatre GLB

**Files :**
- Modify : `tools/car/build-glb.sh` et `tools/car/build-dirty-glb.sh` (sorties `car-cyber-{high,low}.glb` et
  `car-dirty-{high,low}.glb`), `assets/car-src/README.md`, `assets/models/car/LICENCES.md`,
  `crates/cf-world-neon/src/car/systems.rs` (`ModelQuality::path`).
- Create : `assets/models/car/car-cyber-{high,low}.glb` (restaurés du commit parent de `a79b4f3`),
  renommage de `car-{high,low}.glb` en `car-dirty-{high,low}.glb` (`git mv`).
- Test : `crates/cf-world-neon/tests/car.rs`

**Interfaces :**
- Produces : `pub enum CarVariant { Dirty, Cyber }` dans `car/systems.rs`, avec
  `CarVariant::path(self, quality: ModelQuality) -> &'static str` (relatif à `assets/`). `ModelQuality::path` est
  remplacé par cette méthode ; `From<cf_show::persist::CarChoice> for CarVariant`.

- [ ] **Step 1 :** `git show a79b4f3^:assets/models/car/car-high.glb > …/car-cyber-high.glb`, idem pour `low`.
  Mettre la licence de la CyberCar dans `LICENCES.md` telle qu'elle y figurait avant `a79b4f3` (elle est notée
  « inconnue » dans `car/mod.rs` : ne pas la déclarer libre).
- [ ] **Step 2 : test** `les_deux_voitures_se_decodent_en_haute_et_basse_qualite` : les quatre fichiers passent
  `CarModelAsset::from_glb`, et chaque modèle expose les quatre `spin_*`, les deux `steer_*` et un repère de caisse.
  Si la CyberCar n'a pas les mêmes nœuds, **s'arrêter et le noter dans la PR** : le test dit lesquels manquent, et la
  tâche 5 adapte la construction (ne pas renommer des nœuds dans le GLB à la main).
- [ ] **Step 3 :** implémenter `CarVariant`, adapter `frame.rs` et `glb.rs` (chemins de test) au nouveau nommage.
- [ ] **Step 4 :** `cargo nextest run -p cf-world-neon` : vert. **Step 5 : commit**
  `[X-04] Deux voitures versionnées : CyberCar et dirty car, en haute et basse qualité`.

### Task 5 : précharger les deux voitures, changer de voiture à chaud

**Files :**
- Modify : `crates/cf-world-neon/src/car/systems.rs` (`CarModelSource` devient la liste des deux chemins ;
  `load_model` décode les deux dans deux tâches), `crates/cf-world-neon/src/car/mod.rs` (plugin, ressource, système),
  `crates/cf-world-neon/src/car/spawn.rs` (rendre la construction rejouable).
- Create : `crates/cf-world-neon/src/car/switch.rs` (< 200 lignes)
- Test : `crates/cf-world-neon/src/car/switch.rs` (tests en bas) et `crates/cf-world-neon/tests/car.rs`

**Interfaces :**
- Consumes : `CarVariant` (tâche 4), `Settings` (tâche 2, lecture de `car` au démarrage).
- Produces :
  - `#[derive(Resource)] pub struct CarChoiceRequest(pub CarVariant)` : écrite par l'opérateur ; la voiture en place
    est celle de la ressource `ActiveCar(pub CarVariant)`.
  - `pub(super) fn switch_car(...)` en `CfSet::Simulate`, avant `drive_car` : si `CarChoiceRequest != ActiveCar`,
    détruit récursivement les enfants de la racine (caisse, roues, lumières, écrans, traînée, fumée de l'ancien
    `CarRig`), puis reconstruit avec le `CarModelAsset` préchargé de l'autre variante. Conserve `AbsolutePosition`, le
    `CarState` mesuré (vitesse, `previous`) et la teinte courante.
  - Étape de chargement : la tâche de chargement se ferme quand **les deux** modèles sont décodés ; un modèle refusé
    n'empêche pas l'autre (repli : on garde la variante saine et on désactive le choix de l'autre).

- [ ] **Step 1 : tests** : `changer_de_voiture_detruit_toutes_les_entites_de_l_ancienne` (compter les entités avec
  `CarPart` ou enfants de la racine avant/après, doit être égal à celui d'une construction neuve),
  `la_position_et_la_vitesse_sont_gardees_au_changement`, `un_modele_refuse_garde_la_voiture_saine`,
  `demander_la_voiture_en_place_ne_reconstruit_rien`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter (taguer avec un composant marqueur `CarPart` tout ce que
  `spawn_car` et `effects::spawn_effects` créent, pour la destruction). **Step 4 :** lancer, vert.
- [ ] **Step 5 :** vérifier qu'aucun `CarModelAsset` n'est décodé après `Startup` (test : le `AsyncComputeTaskPool` ne
  reçoit aucune tâche au changement). **Step 6 : commit** `[X-05] Changement de voiture à chaud, sans chargement`.

### Task 6 : registre des événements du monde

**Files :**
- Create : `crates/cf-render/src/events_catalog.rs` (< 200 lignes)
- Modify : `crates/cf-render/src/lib.rs`, et un enregistrement par famille :
  `crates/cf-world-neon/src/traffic/mod.rs` (famille « Trafic », depuis `EventName::ALL`),
  `crates/cf-world-neon/src/stunts/mod.rs` (famille « Cascades », depuis les scripts de `stunts/scripts/`),
  `crates/cf-world-neon/src/landmarks/plugin.rs` (famille « Immeubles et repères »),
  `crates/cf-world-neon/src/weather/` (famille « Météo »).
- Test : `crates/cf-render/src/events_catalog.rs` (tests en bas), `crates/cf-world-neon/tests/` (un test de complétude).

**Interfaces :**
- Produces :
  - `pub struct WorldEvent { pub id: &'static str, pub label: &'static str, pub family: &'static str, pub note:
    &'static str }` (`note` : une ligne de description, `""` si aucune).
  - `#[derive(Resource, Default)] pub struct WorldEventCatalog { events: Vec<WorldEvent> }` avec
    `register(&mut self, WorldEvent)` (ignore un `id` déjà présent), `families(&self) -> Vec<&str>` (ordre
    d'enregistrement), `in_family(&self, &str) -> impl Iterator<Item=&WorldEvent>`,
    `search(&self, &str) -> Vec<&WorldEvent>` (insensible à la casse, sur `id`, `label` et `family`).
  - `pub trait WorldEventAppExt { fn register_world_events(&mut self, events: &[WorldEvent]) -> &mut Self; }`.

- [ ] **Step 1 : tests** : `un_id_en_double_est_ignore`, `les_familles_gardent_l_ordre_d_enregistrement`,
  `la_recherche_ignore_la_casse_et_cherche_dans_la_famille`, et dans `cf-world-neon` :
  `chaque_evenement_du_trafic_est_dans_le_catalogue` (compare `EventName::ALL` au catalogue, pour que l'ajout d'un
  événement sans entrée de catalogue fasse échouer le test).
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter le registre et l'enregistrement par famille. Pour les
  familles autres que le trafic, ne lister que ce qui se déclenche réellement par `Action::Trigger` aujourd'hui ;
  un événement qui n'a pas de point d'entrée de déclencheur est repéré et **noté dans la PR « Écarts »**, pas
  inventé. Ajouter dans chaque famille le point d'entrée manquant seulement s'il tient en quelques lignes et passe par
  le limiteur.
- [ ] **Step 4 :** lancer, vert. **Step 5 : commit** `[X-06] Catalogue des événements du monde par famille`.

### Task 7 : onglet « Debug monde » dans la fenêtre opérateur

**Files :**
- Create : `crates/cf-operator/src/debug_panel.rs` (< 300 lignes)
- Modify : `crates/cf-operator/src/lib.rs` (déclarer le module), `crates/cf-operator/src/ui.rs` (ajouter l'onglet au
  panneau central hors vue Live et hors chargement), `crates/cf-operator/src/live/commands.rs` si les ordres de l'écran
  Live doivent connaître l'onglet (sinon rien).
- Test : `crates/cf-operator/src/debug_panel.rs` (tests en bas, logique de filtrage et d'ordres sans egui)

**Interfaces :**
- Consumes : `WorldEventCatalog` (tâche 6), `CarChoiceRequest` / `ActiveCar` (tâche 5), `Settings` (tâche 2),
  `LiveOrders` (file des `Action` de l'opérateur).
- Produces :
  - `pub(crate) fn draw(ui, catalog: &WorldEventCatalog, state: &mut DebugState, active: CarVariant, orders: &mut
    Vec<Action>, car_request: &mut Option<CarVariant>)`.
  - `DebugState { filter: String, open_families: BTreeSet<String>, last_fired: Option<(String, f64)> }`.
  - `fn events_to_show(catalog, filter) -> Vec<(family, Vec<&WorldEvent>)>` pure, testée.

- [ ] **Step 1 : tests** : `le_filtre_garde_les_familles_qui_ont_une_correspondance`,
  `un_clic_sur_lancer_ecrit_une_action_trigger_avec_l_id_de_l_evenement`,
  `deux_clics_dans_la_meme_image_ecrivent_au_plus_deux_ordres` (la file `orders` est bornée à 8 par image),
  `choisir_la_voiture_en_place_n_ecrit_rien`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter l'écran : en haut le sélecteur de voiture (deux boutons
  radio) ; dessous un champ de recherche ; puis une `CollapsingHeader` par famille avec, pour chaque événement, son
  libellé, sa note en grisé et un bouton « Lancer ». Le dernier événement lancé s'affiche en haut pendant 3 s.
  Un changement de voiture écrit aussi `Settings.value.car`. Les familles ouvertes sont gardées dans
  `Settings.value.collapsed_panels` (tâche 3).
- [ ] **Step 4 :** lancer, vert. **Step 5 :** lancer l'application, ouvrir l'onglet, lancer un événement de chaque
  famille et changer la voiture ; relever dans la PR ce qui a été vu (captures avec `Xvfb` en cloud, sinon à l'écran).
- [ ] **Step 6 : commit** `[X-07] Onglet Debug monde : lancer les événements, changer la voiture`.

### Task 8 : vérification globale et livraison

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo nextest run --workspace`, `cargo test --workspace --doc` : tout vert, sortie résumée dans la PR.
- [ ] Mettre à jour `docs/` du guide opérateur (`docs/` contient le guide de J9-04) avec une section « Debug monde » et
  « Réglages sauvegardés » (emplacement du fichier, comment le remettre à zéro : supprimer `settings.ron`).
- [ ] Pousser la branche et ouvrir la PR `[X] Réglages sauvegardés, choix de la voiture, onglet Debug monde` avec les
  sections « Critères », « Preuves » et « Écarts ».
