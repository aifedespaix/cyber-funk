# R2 · Sonde de test (MCP) : plan d'implémentation

> **Pour les agents :** sous-compétence requise : `superpowers:subagent-driven-development` (recommandé) ou `superpowers:executing-plans`. Les étapes utilisent des cases `- [ ]`.

**Goal :** un serveur MCP (`cf-probe`) qui lance ou rejoint cyber-funk, envoie des `Action`, capture, lit l'état et le journal, et mesure la synchronisation musique/image en millisecondes.

**Architecture :** `cf-probe-proto` (types serde, analyse de synchro, sans Bevy) ; `cf-probe` (binaire MCP stdio, sans Bevy, gère le processus) ; `ProbePlugin` dans `crates/cyber-funk/src/probe/` derrière `feature = "probe"` (thread TCP local ↔ systèmes Bevy par canaux).

**Tech Stack :** Rust 2024, Bevy en place, `rmcp` (à vérifier avec Context7), `serde`/`serde_json`, `tokio` seulement si `rmcp` l'impose (côté `cf-probe` uniquement), canaux `std::sync::mpsc`.

**Spec :** `docs/superpowers/specs/2026-11-22-sonde-de-test-design.md`. Branche : `task/R2-sonde`.

## Contraintes globales

- Pas de `unwrap`/`expect`/`panic!` hors tests ; erreurs `thiserror` (libs), `anyhow` (binaire). Fichiers < 400 lignes. Commentaires en français.
- `cf-probe-proto` et `cf-probe` ne dépendent pas de Bevy. Feature `probe` absente par défaut et de la distribution.
- Réseau : 127.0.0.1 uniquement. Le thread réseau ne touche jamais le monde Bevy.
- `send_action` passe par `ControlAction` (même chemin que le clavier) : le limiteur de flashs (règle 7) reste actif.
- Avant chaque commit : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace`. Itérer par crate (`-p`), `CARGO_INCREMENTAL=0`.
- Dernières versions stables ; lire la doc de `rmcp` avec Context7 avant la tâche 4.

## Focus de relecture

- Requête JSON malformée, trop longue ou ligne coupée : ignorée, comptée, jamais de panic ni de blocage (tâche 2).
- `send_action` avec une `Action` inconnue ou une valeur hors bornes (`Master(2.0)`) : refus explicite, pas de clamp silencieux (tâche 2).
- Application morte ou plantée pendant un appel : l'outil répond par une erreur claire, le MCP reste vivant et `log` marche (tâche 5).
- Deux clients ou un client qui se déconnecte en pleine requête : le plugin continue d'écouter (tâche 2).
- Piste sans kick détecté (silence) : `measure_sync` renvoie un verdict « pas de données », pas un retard nul (tâche 6).

---

### Tâche 1 : `cf-probe-proto`

**Files :** Create `crates/cf-probe-proto/{Cargo.toml,src/lib.rs,src/message.rs}` ; Modify `Cargo.toml` (membre du workspace, dépendance `cf-probe-proto`).

**Interfaces :**
- Consumes : `cf_control::Action` (serde).
- Produces :
  - `enum ProbeRequest { Ping, SendAction(Action), State, Capture { name: String }, Step { frames: u32 }, StartSyncRecording, StopSyncRecording }`
  - `enum ProbeReply { Pong, Ack, Error(String), State(StateSnapshot), Captured { path: String }, Trace(SyncTrace) }`
  - `struct StateSnapshot { sim_time_s: f64, phase: String, cue: Option<String>, shot: Option<String>, beat_phase: f32, bar_phase: f32, grid_beat: f64, grid_trust: f32, kick_count: u32, kick_now: bool, fps: f32 }`
  - `struct SyncSample { t_s: f64, kick_now: bool, beat_phase: f32, grid_trust: f32, luminance: f32 }`, `struct SyncTrace { samples: Vec<SyncSample> }`
  - `fn encode_line(&impl Serialize) -> String` et `fn decode_line<T: DeserializeOwned>(&str) -> Result<T, ProtoError>` (un JSON par ligne, `\n` final).
  - `const MAX_LINE_BYTES: usize = 64 * 1024`.

- [ ] **Étape 1 : tests rouges** dans `message.rs` : `aller_retour_de_chaque_requete` (toutes les variantes), `aller_retour_d_une_action_quelconque` (proptest sur `Action`, si `Action` n'a pas d'`Arbitrary`, balayer un échantillon écrit à la main couvrant chaque famille), `une_ligne_trop_longue_est_refusee` (`decode_line` sur `MAX_LINE_BYTES + 1` octets → `ProtoError::TooLong`), `un_json_invalide_est_une_erreur_pas_un_panic`.
- [ ] **Étape 2 :** `cargo test -p cf-probe-proto` : échec attendu (types absents).
- [ ] **Étape 3 :** implémenter les types ci-dessus (`thiserror` pour `ProtoError`).
- [ ] **Étape 4 :** `cargo test -p cf-probe-proto` : vert.
- [ ] **Étape 5 :** commit `R2 : protocole de la sonde (cf-probe-proto)`.

### Tâche 2 : `ProbePlugin`, canal TCP et `send_action`

**Files :** Create `crates/cyber-funk/src/probe/{mod.rs,server.rs,bridge.rs}` ; Modify `crates/cyber-funk/Cargo.toml` (`probe = []`, dépendance `cf-probe-proto`), `crates/cyber-funk/src/main.rs` (ajoute `ProbePlugin` sous `#[cfg(feature = "probe")]`, option `--probe` qui l'active, sans quoi rien n'écoute même compilé).

**Interfaces :**
- Consumes : `ProbeRequest`/`ProbeReply` (tâche 1) ; `cf_render::ControlAction(TimedAction)`.
- Produces : `pub struct ProbePlugin;` ; fichier d'état `probe.json` (`{ "port": u16, "pid": u32 }`) écrit dans le dossier de session (`logging::default_log_dir()`) ; `server::spawn(listener) -> (Receiver<Pending>, JoinHandle)` où `Pending { request: ProbeRequest, reply: Sender<ProbeReply> }`.

- [ ] **Étape 1 : tests rouges** (`probe/tests.rs`, `App` minimale) : `une_action_envoyee_devient_un_message_control_action` (requête `SendAction(Action::Go)` → un `ControlAction` lu, réponse `Ack`) ; `une_valeur_hors_bornes_est_refusee` (`Action::Master` à 2.0 → `Error`, aucun message émis) ; `une_ligne_malformee_est_comptee_et_ignoree` (le serveur répond `Error`, la connexion reste utilisable) ; `un_client_qui_se_deconnecte_ne_bloque_pas_le_plugin` ; `le_port_est_ecrit_dans_probe_json`.
- [ ] **Étape 2 :** `cargo test -p cyber-funk --features probe probe` : échec.
- [ ] **Étape 3 :** `server.rs` : thread qui écoute `127.0.0.1:0`, un thread par client (lecture ligne à ligne bornée par `MAX_LINE_BYTES`), envoie `Pending` par canal et attend la réponse avec un délai (2 s, sinon `Error("délai")`). `bridge.rs` : système `Update` en `CfSet::Input` qui vide les `Pending` (`try_recv`, jamais bloquant) et traduit `SendAction` en `ControlAction` (horodaté par le temps de simulation). Validation des bornes avant émission : réutiliser les constructeurs bornés de `cf-control` si présents, sinon refuser toute valeur hors 0..1.
- [ ] **Étape 4 :** tests verts.
- [ ] **Étape 5 :** commit `R2 : canal de contrôle local et send_action`.

### Tâche 3 : `state`, `capture` et `step`

**Files :** Create `crates/cyber-funk/src/probe/{snapshot.rs,capture.rs}` ; Modify `probe/bridge.rs`.

**Interfaces :**
- Consumes : tâche 2 (`Pending`), `Option<Res<FilteredSignals>>`, `RunPhase`, `Screenshot`/`save_to_disk` (voir `rehearse.rs`), `TimeUpdateStrategy`.
- Produces : réponses `State(StateSnapshot)`, `Captured { path }`, `Ack` pour `Step`.

- [ ] **Étape 1 : tests rouges** : `l_instantane_reflete_les_signaux_filtres` (on pose `grid_trust = 0.7`, `kick_count = 3` dans le monde → le snapshot les porte) ; `step_avance_de_n_images_au_pas_fixe` (`Step { frames: 10 }` → 10 mises à jour, temps de simulation avancé de 10 pas) ; `capture_refuse_un_nom_avec_chemin` (`name = "../x"` → `Error`) ; `capture_ecrit_dans_captures_sonde` (nom `a` → chemin `captures/probe/a.png`, la capture est demandée une fois).
- [ ] **Étape 2 :** échec attendu.
- [ ] **Étape 3 :** `snapshot.rs` : `fn snapshot(world_params…) -> StateSnapshot` (cue et plan par leurs noms français du catalogue). `capture.rs` : spawn de `Screenshot::primary_window()` + `save_to_disk`, nom nettoyé (`[A-Za-z0-9_-]` seulement). `Step` : bascule en `TimeUpdateStrategy::ManualDuration(pas)` (pas de 0,05 s comme `--rehearse`, surchargeable par `CF_PROBE_STEP_S`) et rend la réponse après N images.
- [ ] **Étape 4 :** tests verts.
- [ ] **Étape 5 :** commit `R2 : état, capture et pas de temps fixe`.

### Tâche 4 : serveur MCP `cf-probe` (squelette et outils de lecture/écriture)

**Files :** Create `crates/cf-probe/{Cargo.toml,src/main.rs,src/mcp.rs,src/client.rs}` ; Modify `Cargo.toml` (membre).

**Interfaces :**
- Consumes : `cf-probe-proto` ; `probe.json` (tâche 2).
- Produces : `client::ProbeClient { fn connect(port) -> Result<Self>; fn call(&mut self, ProbeRequest) -> Result<ProbeReply> }` ; outils MCP `attach` (lit `probe.json`, se connecte), `send_action` (JSON d'`Action` → `SendAction`), `state`, `capture`, `step`.

- [ ] **Étape 1 :** `mcp__context7__resolve-library-id` puis `query-docs` pour `rmcp` (déclaration d'outils, transport stdio, version) ; noter la version retenue dans `Cargo.toml`.
- [ ] **Étape 2 : tests rouges** contre une fausse app (écouteur TCP dans le test) : `send_action_transmet_l_action_et_rend_ack` ; `state_rend_le_snapshot` ; `un_outil_sans_attach_dit_d_attacher_d_abord` ; `une_action_json_invalide_est_une_erreur_d_outil_pas_un_plantage`.
- [ ] **Étape 3 :** implémenter `client.rs` (délai de lecture 3 s) et `mcp.rs` (un outil par fonction, schémas dérivés par serde/schemars selon `rmcp`) ; `main.rs` sert en stdio.
- [ ] **Étape 4 :** tests verts ; essai manuel : `cf-probe` listé par `claude mcp add -s local cf-probe -- cargo run -q -p cf-probe` (hors dépôt, noté dans la doc de la tâche 7).
- [ ] **Étape 5 :** commit `R2 : serveur MCP cf-probe (attach, send_action, state, capture, step)`.

### Tâche 5 : `launch`, `stop` et `log` (survie aux plantages)

**Files :** Create `crates/cf-probe/src/{process.rs,logs.rs}` ; Modify `mcp.rs`.

**Interfaces :**
- Consumes : `ProbeClient` (tâche 4).
- Produces : `process::Launched { child, port }` ; `fn launch(opts: LaunchOptions) -> Result<Launched>` (`LaunchOptions { show: Option<PathBuf>, fixed_step: bool, software_render: bool, extra_args: Vec<String> }`, lance `cargo run -p cyber-funk --features probe -- --probe …` ou le binaire déjà compilé s'il existe) ; outils `launch`, `stop` ; `logs::run(args) -> String` qui exécute `python tools/debuglog/dbg.py <sous-commande> --limit N` (réutilise `dbg.py`, pas de seconde lecture du journal) ; outil `log`.

- [ ] **Étape 1 : tests rouges** : `launch_attend_probe_json_puis_se_connecte` (faux binaire qui écrit `probe.json` et écoute) ; `launch_echoue_proprement_si_l_app_meurt_au_demarrage` (faux binaire qui sort en code 1 → erreur avec la fin de sa sortie) ; `un_appel_apres_la_mort_de_l_app_rend_une_erreur_claire_et_log_reste_utilisable` ; `stop_tue_le_processus_enfant` ; `log_transmet_la_sous_commande_et_borne_la_sortie`.
- [ ] **Étape 2 :** échec attendu.
- [ ] **Étape 3 :** `process.rs` : délai de démarrage configurable (120 s par défaut : première compilation), surveillance `try_wait` à chaque appel. `logs.rs` : liste blanche de sous-commandes (`crash`, `summary`, `timeline`, `find`, `spans`, `check`, `sessions`), sortie tronquée à `limit` lignes.
- [ ] **Étape 4 :** tests verts.
- [ ] **Étape 5 :** commit `R2 : launch, stop et log, le MCP survit aux plantages`.

### Tâche 6 : `measure_sync`

**Files :** Create `crates/cf-probe-proto/src/sync_report.rs`, `crates/cyber-funk/src/probe/sync_recorder.rs` ; Modify `mcp.rs`, `probe/bridge.rs`.

**Interfaces :**
- Consumes : `SyncTrace`/`SyncSample` (tâche 1) ; `Step` (tâche 3).
- Produces : `fn analyze(trace: &SyncTrace, thresholds: &SyncThresholds) -> SyncReport` ; `struct SyncReport { kicks: u32, mean_delay_ms: Option<f64>, jitter_ms: Option<f64>, phase_drift_ms_per_min: Option<f64>, trust_rise_s: Option<f64>, verdict: Verdict }` ; `enum Verdict { Ok, TooLate, TooJittery, Drifting, NoData }` ; `struct SyncThresholds { max_delay_ms: f64, max_jitter_ms: f64, max_drift_ms_per_min: f64 }` avec `Default` (valeurs de départ : 60, 25, 40, à valider au premier relevé).

- [ ] **Étape 1 : tests rouges** (signal synthétique, sans app) : `retrouve_un_retard_connu` (kicks toutes les 0,5 s, pic de luminance 80 ms après → `mean_delay_ms ≈ 80 ± 3`, verdict `TooLate` avec seuil 60) ; `la_gigue_est_l_ecart_type_des_retards` ; `une_derive_de_phase_est_detectee` ; `sans_kick_le_verdict_est_pas_de_donnees` (jamais un retard nul) ; `trust_rise_est_le_premier_instant_au_dessus_de_0_6` ; `un_pic_de_luminance_manquant_ignore_ce_kick_sans_fausser_la_moyenne`.
- [ ] **Étape 2 :** échec attendu.
- [ ] **Étape 3 :** implémenter `analyze` (pour chaque kick, pic de luminance dans les 250 ms suivantes). `sync_recorder.rs` : à `StartSyncRecording`, un système de `CfSet::Present` pousse un `SyncSample` par image dans un `Vec` réservé à l'avance (pas d'allocation par image, règle 4) ; la luminance vient de la valeur uniforme du post-process (repérer le champ dans `cf-render` ; si absente, l'exposer en lecture seule sous `feature = "probe"`). `StopSyncRecording` rend la `Trace`.
- [ ] **Étape 4 :** outil `measure_sync { track: Option<String>, seconds: u32 }` : lance la piste de démo en pas fixe, enregistre, analyse, rend le rapport. Tests verts.
- [ ] **Étape 5 :** commit `R2 : mesure de synchronisation musique/image`.

### Tâche 7 : documentation, premier relevé, clôture

**Files :** Create `docs/sonde.md` ; Modify `docs/worldbuilding/CHANTIERS.md` (journal R2, ligne R2), `docs/rythme.md` (renvoi vers `docs/sonde.md`).

- [ ] **Étape 1 :** `docs/sonde.md` : installation (`claude mcp add -s local cf-probe …` avec `MSYS_NO_PATHCONV=1`), chaque outil avec un exemple, comment mesurer, seuils.
- [ ] **Étape 2 :** relevé réel sur la machine : `launch` → `measure_sync` sur une piste de démo ; consigner le rapport dans `docs/sonde.md` et `docs/captures/R2/`. Si aucune grille connue n'existe pour les pistes de démo, ajouter `assets/music/<piste>.grid.ron` (tempo, décalage) et le dire.
- [ ] **Étape 3 :** journal R2 dans `CHANTIERS.md` (mêmes rubriques que R1 : fait, écarts, prochaine session) ; cocher les critères dans la PR avec preuves ; dire ce qui n'a pas pu être vérifié (carte son, écran réel).
- [ ] **Étape 4 :** `fmt`, `clippy --workspace`, `nextest --workspace` une seule fois ; commit `R2 : documentation et premier relevé`.

---

## Auto-relecture

- Couverture : build A (tâches 1-5), outils `launch/attach/stop/send_action/capture/state/log/step/measure_sync` (2-6), survie aux plantages (5), feature `probe` (2), tests de la spec (chaque tâche), documentation (7).
- Cohérence des types : `Pending`, `ProbeClient`, `SyncTrace`/`SyncReport` ont le même nom partout.
- Écart assumé : la source de luminance (tâche 6) et l'existence d'une grille de référence pour les pistes (tâche 7) se décident à la lecture du code ; la tâche dit quoi faire dans chaque cas.
