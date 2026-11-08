# Refonte de la fenêtre opérateur — plan de réalisation

> **Pour les agents :** utiliser superpowers:subagent-driven-development (recommandé) ou superpowers:executing-plans,
> tâche par tâche. Cases `- [ ]` pour le suivi.

**Objectif :** remplacer l'opérateur actuel par trois onglets (Live, DJs, Debug), une vue Live en trois colonnes, une
playlist gérée et un éditeur de DJ.

**Architecture :** la logique de playlist (réordonner, ajouter, retirer, durées) et le choix de la sortie audio vivent
dans `cf-audio` / `cf-render::audio` (sans Bevy pour `cf-audio`). `cf-operator` n'a plus que de l'affichage egui, un
module par colonne. L'état Démarrer/Arrêter est une ressource provisoire dans `cf-operator`, que le projet 2 branchera
sur le monde 3D.

**Stack :** Rust 2024, Bevy / `bevy_egui`, `cpal`, `symphonia`, `ron`.

**Spec :** `docs/superpowers/specs/2026-11-08-operateur-refonte-design.md` (à lire en entier avant toute tâche).

## Contraintes globales

- Règles 🔴 de `CLAUDE.md` : pas d'`unwrap`/`expect`/`panic!` hors tests ; thread audio sans allocation, verrou, log ni
  I/O ; pas d'allocation par image dans les systèmes d'UI chauds ; `cf-audio`, `cf-show`, `cf-control` sans Bevy ;
  fichiers ≤ 400 lignes ; commentaires et docs en français.
- Avant chaque commit : `cargo fmt --all`, puis clippy et tests **du crate touché** ; clippy `--workspace -D warnings` et
  `cargo nextest run --workspace` une fois, à la dernière tâche. Compiler avec `CARGO_INCREMENTAL=0`.
- Couleur d'un DJ = couleur de son thème. Texte sur la voiture = `DjIdentity::name`. Aucun champ ajouté au schéma RON.
- Une branche : `task/operateur-refonte`. Pas de modification de `docs/TASKS.md`.

## Revue : cas qui risquent de mordre

- Aucun DJ dans l'événement : la colonne DJ affiche « + Nouveau DJ » seul, le bouton Démarrer reste utilisable. (Tâche 5)
- Playlist vide ou entièrement illisible : le lecteur affiche « Aucune piste », précédent/suivant sont inactifs, pas de
  panique. (Tâche 1, 8)
- Fichier ajouté introuvable, corrompu ou non audio : grisé avec la raison, sauté à la lecture. (Tâche 1)
- Retirer ou déplacer le titre en cours : la lecture continue sur ce titre, l'indice courant suit. (Tâche 1)
- Périphérique d'entrée/sortie débranché ou liste vide : message, repli sur le périphérique par défaut. (Tâche 2, 7)
- Sauvegarde du show impossible (lecture seule, aucun show) : bandeau, pas de crash, DJ gardés en mémoire. (Tâche 9)
- Double-clic sur Démarrer pendant `Démarrage` : ignoré. (Tâche 4)
- Ancienne valeur `last_tab` (`settings`, `stats`) dans les préférences : ramenée sur Live. (Tâche 3)

---

### Tâche 1 : Playlist gérée (`cf-audio`)

**Fichiers :**
- Modifier : `crates/cf-audio/src/playlist/mod.rs`, `scan.rs`
- Créer : `crates/cf-audio/src/playlist/probe.rs` (lecture de la durée), `crates/cf-audio/src/playlist/edit.rs`
  (réordonner/ajouter/retirer, pour garder `mod.rs` sous 400 lignes)
- Tests : dans les modules `tests` de ces fichiers

**Interfaces :**
- Produit : `Track.duration_s: Option<f64>` ; `Track.error: Option<String>` (fichier illisible) ;
  `probe::duration_s(path: &Path) -> Result<f64, ProbeError>` ;
  `Playlist::move_track(&mut self, from: usize, to: usize)` ; `Playlist::add(&mut self, track: Track)` ;
  `Playlist::remove(&mut self, index: usize) -> Option<Track>` ; `Playlist::total_s(&self) -> f64` ;
  `Playlist::remaining_s(&self, position_s: f64) -> f64` (reste du titre courant + titres suivants dans l'ordre de la
  liste) ; toutes laissent `index()` pointé sur le même titre quand il n'est pas retiré.

- [ ] **Étape 1 : tests en échec** — `deplacer_conserve_les_pistes_et_suit_le_titre_courant`,
  `retirer_le_titre_courant_charge_le_suivant`, `retirer_le_dernier_titre_laisse_une_liste_vide_sans_panique`,
  `total_est_la_somme_des_durees_connues`, `restant_compte_le_reste_du_titre_courant`,
  `un_fichier_illisible_est_marque_et_saute`. Utiliser `proptest` pour : le multiensemble des pistes est inchangé après
  n'importe quelle suite de déplacements.
- [ ] **Étape 2 : les voir échouer** — `cargo test -p cf-audio playlist`
- [ ] **Étape 3 : implémenter** les signatures ci-dessus. `probe::duration_s` utilise `symphonia` (métadonnées sans
  décoder tout le fichier). L'ajout lit la durée **hors thread audio** ; `scan_music_dir` la renseigne aussi.
- [ ] **Étape 4 : tests au vert** — `cargo test -p cf-audio`
- [ ] **Étape 5 : commit** — `Playlist : réordonner, ajouter, retirer et durées`

### Tâche 2 : Choix de la sortie audio (`cf-audio` + `cf-render::audio`)

**Fichiers :**
- Modifier : `crates/cf-audio/src/player/output/cpal_output.rs`, `crates/cf-render/src/audio/types.rs`
  (+ le système qui traite `AudioCommand`)

**Interfaces :**
- Produit : `AudioCommand::SelectOutput(DeviceSelector)` ; `AudioCommand::RefreshOutputs` ;
  `AudioStatus.outputs: Vec<String>` ; `AudioStatus.output_device: DeviceSelector`. `DeviceSelector` existe déjà.

> État constaté : seul le choix d'entrée existe. Cette tâche ajoute la sortie du lecteur de démo.

- [ ] **Étape 1 : test en échec** — liste de sorties via un backend factice (`ManualOutput`/`NullOutput`) : sélectionner
  une sortie inconnue retombe sur la sortie par défaut et ajoute une note dans `AudioStatus.notes`.
- [ ] **Étape 2 : implémenter** l'énumération des sorties `cpal` et la réouverture du lecteur sur la sortie choisie, en
  reprenant la piste et la position. Rien d'alloué ni de bloquant dans le callback `cpal`.
- [ ] **Étape 3 : vérifier** — `cargo test -p cf-audio -p cf-render audio`
- [ ] **Étape 4 : commit** — `Audio : choix de la sortie du lecteur`

### Tâche 3 : Coque à trois onglets (`cf-operator`)

**Fichiers :**
- Modifier : `crates/cf-operator/src/shell/mod.rs`, `shell/status_bar.rs`, `shell/shortcuts.rs`, `ui.rs`, `lib.rs`
- Tests : `shell/tests.rs`

**Interfaces :**
- Produit : `enum Tab { Live, Djs, Debug }` avec `key()` = `"live" | "djs" | "debug"` ; `Tab::restore(&str) -> Tab`
  renvoie `Live` pour toute valeur inconnue ou ancienne (`settings`, `stats`) ; `OperatorMode` disparaît.
  Raccourcis Ctrl+1/2/3.

- [ ] **Étape 1 : tests en échec** — `restore_ramene_les_anciennes_valeurs_sur_live`, `les_trois_onglets_sont_toujours_presents`
  (y compris Debug), `ctrl_1_2_3_choisissent_les_onglets`.
- [ ] **Étape 2 : implémenter**. Debug n'est plus caché ni verrouillé. La barre d'onglets est un `selectable_label`
  par onglet, avec un onglet actif nettement souligné, et réserve à droite la place du badge (Tâche 10).
- [ ] **Étape 3 : vérifier** — `cargo test -p cf-operator shell`
- [ ] **Étape 4 : commit** — `Opérateur : onglets Live, DJs et Debug`

### Tâche 4 : État du show provisoire

**Fichiers :** Créer `crates/cf-operator/src/run_state.rs` ; test dans le même fichier.

**Interfaces :**
- Produit : `enum ShowRun { Waiting, Starting, Running, Stopping }` ; ressource `ShowRunState { run: ShowRun,
  started_at_s: Option<f64> }` ; `ShowRunState::press(&mut self, now_s: f64)` : `Waiting → Running`, `Running →
  Waiting` (sans animation tant que le projet 2 n'existe pas) ; sans effet pendant `Starting`/`Stopping` ;
  `elapsed_s(&self, now_s: f64) -> Option<f64>`.

- [ ] **Étape 1 : tests en échec** — `demarrer_passe_en_cours`, `arreter_repasse_en_attente`,
  `appui_ignore_pendant_les_etats_transitoires`, `duree_ecoulee_nulle_en_attente`.
- [ ] **Étape 2–4 : implémenter, vérifier** — `cargo test -p cf-operator run_state`
- [ ] **Étape 5 : commit** — `Opérateur : état démarrer / arrêter`

### Tâche 5 : Colonne DJ

**Fichiers :** Créer `crates/cf-operator/src/live/dj_column.rs`. Tests dans le même fichier.

**Interfaces :**
- Consomme : l'événement actif (`DjSlot`, `DjIdentity { name, logo, theme }`), l'ordre de changement de DJ déjà utilisé
  par `live/commands.rs` (réutiliser le message existant, ne pas en créer un).
- Produit : `fn dj_cards(ui, slots: &[DjSlot], active: Option<SlotId>) -> DjColumnEvent` avec
  `enum DjColumnEvent { None, Pick(SlotId), New }` ; `fn theme_color(theme: Option<&Theme>) -> egui::Color32` (couleur
  principale du thème, couleur de l'événement si absent).

- [ ] **Étape 1 : tests en échec** — `theme_color_utilise_le_theme_du_dj`, `theme_color_retombe_sur_celui_de_l_evenement`,
  `liste_vide_ne_propose_que_nouveau`. (La logique de choix de couleur est testable sans egui.)
- [ ] **Étape 2 : implémenter**. Carte = pastille de couleur, miniature du logo s'il existe (chargée une fois, mise en
  cache, jamais à chaque image), nom ; carte active mise en avant. Un clic émet `Pick`.
- [ ] **Étape 3 : vérifier, commit** — `Opérateur : colonne des DJ`

### Tâche 6 : Colonne centrale (état, bouton, stats)

**Fichiers :** Créer `crates/cf-operator/src/live/center.rs`. Tests dans le même fichier.

**Interfaces :**
- Consomme : `ShowRunState` (T4), `AudioFeatures`, `ShowState` (section), diagnostics FPS déjà exposés.
- Produit : `fn center(ui, run: &ShowRunState, stats: &LiveStats, dj_name: Option<&str>, now_s: f64) -> bool`
  (vrai si le bouton est pressé) ; `struct LiveStats { bpm: Option<f32>, section: String, fps: f32, level: f32 }` ;
  `fn button_spec(run: ShowRun) -> ButtonSpec { label, color, enabled }` : vert « ▶ DÉMARRER » en `Waiting`, rouge
  « ■ ARRÊTER » en `Running`, inactif sinon.

- [ ] **Étape 1 : tests en échec** — `button_spec_par_etat` (4 états), `bpm_absent_affiche_un_tiret`.
- [ ] **Étape 2 : implémenter** carte d'état + grand bouton + tuiles BPM / Section / FPS / niveau (barres).
- [ ] **Étape 3 : vérifier, commit** — `Opérateur : colonne centrale`

### Tâche 7 : Colonne audio (source, entrée, sortie, MIDI)

**Fichiers :** Créer `crates/cf-operator/src/live/audio_column.rs`.
Modifier : `crates/cf-render` (ressource d'activation MIDI) et sa mémorisation dans les réglages.

**Interfaces :**
- Consomme : `AudioStatus`, `AudioCommand` (dont `SelectSource`, `SelectDevice`, `SelectOutput` de T2).
- Produit : `fn audio_header(ui, status: &AudioStatus, midi: &mut bool) -> Vec<AudioCommand>` ; ressource
  `MidiEnabled(bool)` mémorisée, qui coupe l'écoute du contrôleur seulement (horloge et profils inchangés).
  Mode Micro : sélecteur d'entrée affiché. Mode Musiques : non affiché.

- [ ] **Étape 1 : tests en échec** — `musiques_n_affiche_pas_l_entree`, `micro_affiche_l_entree`,
  `changer_de_source_emet_select_source`, `midi_coupe_le_controleur_et_pas_l_horloge`.
- [ ] **Étape 2 : implémenter**. Source et sortie sur la même ligne, MIDI à côté.
- [ ] **Étape 3 : vérifier, commit** — `Opérateur : colonne audio`

### Tâche 8 : Lecteur et playlist

**Fichiers :** Créer `crates/cf-operator/src/live/playlist_view.rs`, `live/ playlist_store.rs` (liste de session
mémorisée dans les réglages). Ajouter la dépendance `rfd` (sélecteur de fichiers) au workspace, dernière version stable.

**Interfaces :**
- Consomme : `Playlist` (T1), `AudioStatus.tracks`, `AudioCommand::{DemoPrev, DemoToggle, DemoNext, DemoSelect}`.
- Produit : `fn player(ui, status, position_s) -> Vec<AudioCommand>` ; `fn playlist(ui, list: &mut PlaylistModel)
  -> PlaylistEvent` avec `enum PlaylistEvent { None, Play(usize), Moved{from,to}, Removed(usize), AddFiles }` ;
  `fn format_total(secs: f64) -> String` (`h:mm:ss` ou `m:ss`).
  Liste dans un `ScrollArea` ; poignée ⠿ pour glisser ; durée par titre ; pied « N titres · durée totale · restant ».
  Cache des lignes reconstruit seulement quand la liste change.

- [ ] **Étape 1 : tests en échec** — `format_total_gere_heures_et_minutes`, `liste_vide_desactive_precedent_suivant`,
  `la_liste_memorisee_est_relue_et_ignore_les_fichiers_disparus`.
- [ ] **Étape 2 : implémenter**, y compris Aléatoire / Boucle (`PlayMode`).
- [ ] **Étape 3 : vérifier, commit** — `Opérateur : lecteur et playlist`

### Tâche 9 : Onglet DJs (éditeur)

**Fichiers :** Créer `crates/cf-operator/src/tabs/djs/{mod.rs,list.rs,form.rs,save.rs}` en repartant de
`tabs/settings/dj_list.rs` et `dj_form.rs` (les déplacer, ne pas dupliquer).

**Interfaces :**
- Produit : `fn show(ui, state: &mut DjsState, event: &mut Event) -> Vec<DjEdit>` ;
  `enum DjEdit { Create, Duplicate(SlotId), Delete(SlotId), Move{from,to}, Changed(SlotId) }` ;
  `save::write_event(path: &Path, event: &Event) -> Result<(), SaveError>` (écriture atomique : fichier temporaire puis
  renommage) ; l'enregistrement a lieu à chaque modification validée par `cf-show::validate`.
  Champs : nom, thème (parmi les thèmes existants), logo (fichier image, chemin gardé tel quel).

- [ ] **Étape 1 : tests en échec** — `aller_retour_ron_apres_edition`, `nom_trop_long_refuse_et_non_enregistre`,
  `suppression_demande_confirmation`, `sans_show_charge_garde_en_memoire_et_signale`,
  `fichier_en_lecture_seule_signale_sans_panique`.
- [ ] **Étape 2 : implémenter, vérifier, commit** — `Opérateur : onglet DJs`

### Tâche 10 : Badge de mise à jour

**Fichiers :** Modifier `shell/mod.rs` ; supprimer l'usage de `tabs/settings/update_panel.rs` en gardant ses types
`UpdateStatus` et `InstallUpdate` (exportés par `lib.rs`).

**Interfaces :** `fn update_badge(ui, status: &UpdateStatus) -> bool` (vrai si « Installer » est cliqué) ; affiché à
droite des onglets seulement si une mise à jour est disponible, en orange.

- [ ] **Étape 1 : test en échec** — `pas_de_badge_sans_mise_a_jour`, `clic_emet_install_update`.
- [ ] **Étape 2 : implémenter, vérifier, commit** — `Opérateur : badge de mise à jour`

### Tâche 11 : Debug absorbe les statistiques

**Fichiers :** Modifier `crates/cf-operator/src/tabs/debug/mod.rs` ; déplacer `tabs/stats/{charts,diagnostics,
history,log_view}.rs` dans une section repliable de Debug ; supprimer l'onglet Stats.

- [ ] **Étape 1 : adapter les tests** de `tabs/stats/tests.rs` à leur nouvel emplacement ; test
  `debug_contient_graphes_et_journal`.
- [ ] **Étape 2 : implémenter, vérifier, commit** — `Opérateur : Debug regroupe les statistiques`

### Tâche 12 : Assemblage de la vue Live

**Fichiers :** Modifier `live/mod.rs`, `ui.rs`. Tests : `live/tests.rs`.

- [ ] **Étape 1 : test en échec** — `la_vue_live_emet_les_commandes_des_trois_colonnes` (clic DJ → ordre de DJ ; bouton →
  `ShowRunState.press` ; lecteur → `AudioCommand`).
- [ ] **Étape 2 : implémenter**. Trois colonnes (22 % / reste / 30 %) ; sous une largeur minimale, la colonne DJ
  devient un bandeau horizontal.
- [ ] **Étape 3 : vérifier** — lancer l'application (`cargo run -p cyber-funk`), regarder les trois onglets, une
  capture de la vue Live dans `docs/captures/`. Dire dans la PR ce qui n'a pas pu être vérifié.
- [ ] **Étape 4 : commit** — `Opérateur : vue Live en trois colonnes`

### Tâche 13 : Suppression du code retiré

**Fichiers :** supprimer de `cf-operator` : `prepare/`, `tabs/pad*`-liés, `live/holds.rs`, panneaux de sécurité, modes
Auto/Semi/Manuel et `OperatorMode`, `tabs/settings/{audio_panel,controls_panel,learn_panel,profiles,restore (partiel)}`
selon ce que plus rien n'appelle. Garder l'écran de rendu (`screens_view.rs`) dans un petit menu en haut à droite.

- [ ] **Étape 1 :** compiler (`cargo build -p cf-operator`) ; supprimer chaque module orphelin signalé comme mort par
  `cargo clippy -p cf-operator -- -D warnings`. Si `cf-show::director` garde un concept de mode sans appelant, le
  supprimer aussi.
- [ ] **Étape 2 :** mettre à jour `docs/guide-operateur.md` (nouvelles sections Live / DJs / Debug, plus de pads ni de
  modes) et `docs/ARCHITECTURE.md` si une section citée change.
- [ ] **Étape 3 : vérification complète** — `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features
  -- -D warnings`, `cargo nextest run --workspace`. Tout au vert.
- [ ] **Étape 4 : commit** — `Opérateur : retrait des panneaux obsolètes`

## Auto-relecture

- Couverture du spec : onglets (T3), badge (T10), colonne DJ (T5), centre (T6), audio (T7, T2), playlist (T1, T8),
  éditeur DJ (T9), Debug (T11), assemblage (T12), suppressions et guide (T13), états provisoires (T4). Complet.
- Point à vérifier à l'exécution : la sortie audio n'existait pas (T2 l'ajoute) et `rfd` n'est pas encore une dépendance
  (T8 l'ajoute) ; ces deux ajouts dépassent le « UI seulement » annoncé dans le spec.
