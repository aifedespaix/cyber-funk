# Graffitis et projecteurs holographiques Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** La ville porte des graffitis muraux (surtout en banlieue) et des projecteurs holographiques (une barre qui projette un texte néon en l'air) posés automatiquement ou à la main, à partir de mots et de polices modifiables.

**Architecture:** Des données pures (`words.ron`, `placements.ron`, tirages par hachage) décident quoi et où ; une cuisson unique au chargement (`cosmic-text`, comme `signs/bake.rs`) produit deux atlas (mural couleur, hologramme masque + halo). Les murs sont peints par un module du shader de façade (bits libres du `MeshTag`) ; les projecteurs sont un pool d'entités préallouées (modèle `scenery/arches.rs`) dont la lumière passe par le limiteur de flashs existant.

**Tech Stack:** Rust 2024, Bevy 0.20, WESL, `cosmic-text` 0.19, RON, bevy_egui, `cargo nextest`.

**Spec:** `docs/superpowers/specs/2026-11-03-graffitis-design.md`. Contexte : `docs/superpowers/specs/2026-11-01-quartiers-moteur-design.md`.

## Global Constraints

- Règles de `CLAUDE.md` : 4 (pas d'allocation par image en régime établi), 6 (pas de `NoFrustumCulling`), 7 (tout flash ou forte variation de luminance passe par le limiteur ; ≤ 3 changements par seconde), 8 (cuisson et shaders au chargement, jamais pendant un show), 11 (fichiers ≤ 400 lignes), 2 (pas de `unwrap`/`expect` hors tests).
- Commentaires et docs en français, identifiants en anglais.
- Le golden `crates/cf-world-neon/tests/golden/scenery.json` reste identique pour le centre néon par défaut (aucun graffiti configuré = génération inchangée).
- Plafond : 12 projecteurs actifs en même temps, tous types et tailles confondus.
- Trois classes de taille de projecteur, valeurs de départ : petit (barre 2 m, lettres 1 m), moyen (barre 6 m, lettres 3 m), grand (barre 20 m, lettres 10 m).
- Densités de départ (1 immeuble sur N) : mural banlieue 3, résidentiel 10, centre 25 ; projecteur automatique centre 10, banlieue 18, résidentiel quasi nul.
- Apparition d'un projecteur ≤ 1,5 s ; instabilité par tranches ≤ 3 changements par seconde, proportionnelle au niveau accordé par le limiteur (à 0 : stable).
- Fichiers de données : `assets/graffiti/words.ron`, `assets/graffiti/placements.ron`, `assets/fonts/LICENCES.md`. Une entrée invalide est ignorée avec un avertissement ; une police absente ou sans ligne dans `LICENCES.md` saute ses motifs ; le dépôt démarre quand même.
- Avant chaque commit : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, tests verts (itérer par `-p cf-world-neon`, workspace une fois avant de pousser). Compiler avec `CARGO_INCREMENTAL=0`.

## Review Focus

- Police introuvable, `words.ron` vide ou une entrée cassée : l'app démarre, les autres mots fonctionnent (Task 1, Task 4).
- Un projecteur manuel qui chevauche un automatique, ou qui traverse un immeuble : l'automatique est retiré ; le manuel hors route ou à hauteur négative est ignoré avec avertissement (Task 2).
- Plus de 12 projecteurs candidats dans la fenêtre : les plus proches de la voiture gagnent, aucun clignotement d'un slot à l'autre quand la voiture avance (Task 5).
- Flashs coupés par le show (`permitted() == false`) ou panne du limiteur : l'hologramme est stable, jamais éteint d'un coup (Task 6).
- Changement de quartier avec des graffitis dans le champ : rien ne disparaît d'un coup (Task 2).
- Qualité minimale : plus de gros coût, mais la scène reste complète (Task 7).

---

### Task 1: Polices, licences et modèle de données des mots

**Files:**
- Create: `assets/fonts/LICENCES.md`, `assets/fonts/*.ttf|otf` (seulement les fichiers de police utilisés, extraits des archives de `assets/font/`), `assets/graffiti/words.ron` (20 à 30 mots cyberpunk), `assets/graffiti/placements.ron` (quelques exemples, un par taille)
- Create: `crates/cf-world-neon/src/graffiti/mod.rs`, `graffiti/words.rs`, `graffiti/placements.rs`, `graffiti/tests.rs`
- Modify: `crates/cf-world-neon/src/lib.rs` (déclarer `pub mod graffiti;`), `.gitignore` (ignorer `/assets/font/`, les archives)

**Interfaces:**
- Produces :
  - `pub enum GraffitiStyle { Fill, Outline, Extrude }`, `pub enum ProjectorSize { Small, Medium, Large }` avec `fn bar_len_m(self) -> f32` et `fn letter_h_m(self) -> f32` (valeurs des contraintes globales).
  - `pub struct GraffitiWord { pub text: String, pub font: String, pub style: GraffitiStyle, pub wall: bool, pub holo: bool, pub hues: Vec<f32> }`.
  - `pub struct Placement { pub z_m: f64, pub side: i8, pub height_m: f32, pub size: ProjectorSize, pub word: String, pub yaw_deg: f32 }`.
  - `pub fn load_words(ron_text: &str) -> (Vec<GraffitiWord>, Vec<String>)` et `pub fn load_placements(ron_text: &str) -> (Vec<Placement>, Vec<String>)` : valeurs lues + liste d'avertissements lisibles (l'entrée cassée n'invalide pas les autres).
  - `pub fn licensed_fonts(licences_md: &str) -> Vec<String>` : noms de police ayant une ligne remplie dans `LICENCES.md`.

- [ ] **Step 1 : tests** dans `graffiti/tests.rs` : `une_entree_cassee_est_ignoree_et_les_autres_restent`, `un_fichier_vide_donne_zero_mot_sans_erreur`, `les_tailles_ont_les_dimensions_de_la_spec`, `chaque_police_versionnee_a_une_ligne_de_licence` (lit `assets/fonts/` et `LICENCES.md`, compare les deux listes), `le_words_ron_livre_se_charge_sans_avertissement`, `le_placements_ron_livre_se_charge_sans_avertissement`.
- [ ] **Step 2 :** lancer `cargo nextest run -p cf-world-neon graffiti` : échec attendu (module absent).
- [ ] **Step 3 :** implémenter. Lire chaque entrée de liste RON séparément (désérialiser vers `ron::Value` puis convertir) pour qu'une entrée cassée n'en invalide pas d'autres. Copier à la main les `.ttf`/`.otf` utiles dans `assets/fonts/`, écrire `LICENCES.md` (une ligne par police : nom, source, licence CC0 d'après l'utilisateur, date ; `aAnotherTag` : auteur = ami de l'utilisateur, accord à confirmer par lui). Les lignes dont la source est inconnue sont laissées marquées `À COMPLÉTER` : `licensed_fonts` ne les compte pas.
- [ ] **Step 4 :** relancer : vert. **Step 5 : commit** `[X-12] Mots, placements et polices : données et licences`.

### Task 2: Tirages purs (mur et projecteurs) et branchement dans la ville

**Files:**
- Create: `crates/cf-world-neon/src/scenery/graffiti_plan.rs`, `scenery/graffiti_plan_tests.rs`
- Modify: `scenery/districts.rs` (champs de `DistrictLook`), `scenery/layout.rs` (`Part.graffiti`, `Part::tag`, génération), `scenery/config.rs` (`SceneryConfig` porte la bibliothèque), `scenery/mod.rs`

**Interfaces:**
- Consumes : `Row::generate(index, &SceneryConfig)`, `DistrictLook`, `DistrictKind`, `query::collides`, `Hitbox`, `Placement`, `ProjectorSize`.
- Produces :
  - `DistrictLook` gagne `wall_one_in: u32` (0 : jamais) et `holo_one_in: u32` (0 : jamais) avec les valeurs de la section Contraintes ; `Neon` valide le défaut historique seulement si aucune bibliothèque n'est configurée.
  - `Part.graffiti: u8` (0 : aucun, `1..=31` : index du motif mural), écrit dans les bits 27..31 du tag ; valeur 0 = tag inchangé.
  - `pub struct Projector { pub x: f32, pub y: f32, pub z: f64, pub side: i8, pub size: ProjectorSize, pub word: u16, pub yaw_deg: f32, pub manual: bool }`.
  - `pub fn row_projectors(index: i64, config: &SceneryConfig, out: &mut ProjectorList)` où `ProjectorList` est un tableau de taille fixe (aucune allocation) ; fusionne les manuels (priorité) et les automatiques, retire un automatique qui chevauche un manuel, ignore un manuel hors route ou à hauteur négative.
  - `SceneryConfig::set_graffiti(&mut self, lib: GraffitiLibrary)`, `GraffitiLibrary` (mots mur, mots holo, placements) ; par défaut vide.

- [ ] **Step 1 : tests** : `sans_bibliotheque_la_ville_est_identique` (golden `scenery.json` inchangé, `Part.graffiti == 0` partout), `le_meme_index_donne_les_memes_graffitis`, `la_banlieue_porte_plus_de_graffitis_que_le_centre` (sur 2 000 rangées), `le_centre_porte_quelques_projecteurs_et_le_residentiel_presque_aucun`, `la_classe_de_taille_suit_la_hauteur_de_l_immeuble` (grand seulement sur les tours), `un_projecteur_manuel_remplace_l_automatique_qui_le_chevauche`, `un_projecteur_manuel_hors_route_est_ignore`, `un_projecteur_ne_traverse_pas_un_immeuble`, `pendant_un_degrade_le_choix_suit_la_probabilite_croissante` (mêmes graffitis qu'avant le dégradé à probabilité 0, plus aucun mélange abrupt), `row_projectors_n_alloue_pas`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter, hachage de position comme dans `Row::generate` ; le mur ne reçoit un graffiti que sur la zone basse (rez-de-chaussée et premiers étages), un seul par immeuble. **Step 4 :** `cargo nextest run -p cf-world-neon` vert, golden compris ; clippy `-p cf-world-neon`.
- [ ] **Step 5 : commit** `[X-12] Tirages de graffitis et de projecteurs dans la génération`.

### Task 3: Cuisson des atlas

**Files:**
- Create: `crates/cf-world-neon/src/graffiti/bake.rs`, `graffiti/atlas.rs`, `graffiti/bake_tests.rs`
- Modify: `graffiti/mod.rs`, `crates/cf-world-neon/src/lib.rs` (plugin)

**Interfaces:**
- Consumes : `GraffitiWord`, `LoadTracker`, `StepId`, modèle de tâche de `signs/bake.rs`, `cosmic-text` (déjà au workspace).
- Produces :
  - `pub struct GraffitiAtlas` (ressource) : `wall: Handle<Image>` (RGBA), `holo: Handle<Image>` (masque + halo flou, deux versions par mot), `fn wall_cell(&self, index: u8) -> Option<CellRect>`, `fn holo_cell(&self, word: u16) -> Option<HoloCell>`, `bake_s: f32`, `skipped: Vec<String>` (mots sautés avec raison).
  - `pub struct GraffitiPlugin` ajouté par `NeonWorldPlugin` ; cuit dans une tâche de fond inscrite au `LoadTracker` : le show ne s'ouvre qu'après la cuisson.

- [ ] **Step 1 : tests** : `une_police_absente_saute_ses_mots_avec_avertissement`, `une_police_sans_ligne_de_licence_saute_ses_mots`, `la_cuisson_donne_une_cellule_par_mot_garde`, `le_masque_holo_est_monochrome_et_le_halo_est_plus_flou` (variance locale), `la_cuisson_tient_sous_une_seconde_sur_la_liste_livree` (marqué `#[ignore]` s'il dépend de la machine), `aucune_texture_n_est_dessinee_apres_le_startup`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter la cuisson (styles Fill/Outline/Extrude, une cellule par mot, atlas paginé si trop grand). **Step 4 :** vert. **Step 5 : commit** `[X-12] Cuisson des atlas de graffitis`.

### Task 4: Peinture murale dans le shader de façade

**Files:**
- Create: `assets/shaders/cf-world-neon/facade_paint.wesl`
- Modify: `assets/shaders/cf-world-neon/facade.wesl` (appel, import), `crates/cf-world-neon/src/facade/material.rs` (liaison de l'atlas mural, le chemin d'`include_str!` du nouveau module), `facade/mod.rs`, `crates/cf-world-neon/src/world.rs` (relier `GraffitiAtlas` au matériau)
- Test: `crates/cf-world-neon/tests/graffiti_wall.rs`

**Interfaces:**
- Consumes : bits 27..31 du `MeshTag` (`Part::tag`), `GraffitiAtlas::wall_cell`.
- Produces : une fonction WESL `paint_wall(uv: vec2<f32>, face: vec3<f32>, graffiti: u32, base: vec3<f32>) -> vec3<f32>` appelée seulement si `graffiti != 0u` ; un champ du matériau (uniform) qui donne la cellule de chaque index.

- [ ] **Step 1 : test** `tests/graffiti_wall.rs` : un monde minimal avec un immeuble marqué `graffiti = 1` et un autre `0` : le shader compile (même mécanisme que les autres tests de matériaux), l'atlas est lié, l'immeuble `0` est rendu à l'identique de l'état d'avant (comparaison de la sortie de `facade_prepass` ou du hash de pixels, selon l'outil de test existant).
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter le module : décalque dans la zone basse de la façade, échelle et teinte par hachage de la graine, mélange selon la lumière ambiante, jamais de forte variation de luminance. **Step 4 :** vert ; vérifier la compilation WESL sans `shader_defs` supplémentaires. **Step 5 : commit** `[X-12] Graffitis muraux dans la façade`.

### Task 5: Pool de projecteurs et placement

**Files:**
- Create: `crates/cf-world-neon/src/graffiti/projector.rs` (pool, placement), `graffiti/projector_tests.rs`, `tests/graffiti_projectors.rs`
- Modify: `graffiti/mod.rs`, `crates/cf-world-neon/src/world.rs` (créer le pool au `setup`, comme `ArchPool`)

**Interfaces:**
- Consumes : `row_projectors`, `GraffitiAtlas`, `AbsolutePosition`, `WorldLayer`, `fog_range()`, `ArchPool` (modèle).
- Produces : `pub struct ProjectorPool` (ressource, 12 slots × 2 enfants préalloués : barre et texte) ; système `place_projectors` dans `CfSet::Simulate` après les tronçons : recalcule la liste des candidats seulement quand la rangée de la voiture change, garde les 12 plus proches, conserve le slot d'un projecteur déjà affiché (pas de saut d'un slot à l'autre quand la voiture avance).

- [ ] **Step 1 : tests** (`App` Bevy minimal, voir `tests/neon_world.rs`) : `douze_projecteurs_au_plus`, `les_plus_proches_gagnent`, `un_projecteur_garde_son_slot_en_avancant`, `un_slot_libere_est_cache_pas_detruit` (comptage d'entités constant), `place_projectors_n_alloue_pas_en_regime_etabli` (modèle de `tests/signs_no_alloc.rs`).
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter ; chaque projecteur est une entité parent avec deux enfants (barre : pavé fin horizontal à la taille de sa classe ; texte : panneau vertical orienté vers la route, au-dessus de la barre, à la hauteur de lettre de sa classe). **Step 4 :** vert ; clippy. **Step 5 : commit** `[X-12] Pool de projecteurs holographiques`.

### Task 6: Matériau et shader du projecteur, glow, limiteur

**Files:**
- Create: `assets/shaders/cf-world-neon/graffiti_holo.wesl`, `crates/cf-world-neon/src/graffiti/holo_material.rs`, `graffiti/holo_systems.rs`, `graffiti/holo_tests.rs`
- Modify: `graffiti/mod.rs`, `graffiti/projector.rs` (affecter le matériau)

**Interfaces:**
- Consumes : `SignFlash::pulse()` et `permitted()` (`signs/flash.rs`, réutiliser la valeur publiée par le système des enseignes, ne pas créer un second limiteur), `ScenePalette` (couleur d'accent), `AudioFeatures` (intensité lissée), `glyph_h11` et la logique de tranches de `glyph.wesl`.
- Produces : `HoloMaterial` (additif, `AsBindGroup`) avec uniforms `{ tint, glow, appear, level, time }` ; système `drive_holo` qui écrit `level = limiteur(respiration audio)` chaque image sans allocation.

- [ ] **Step 1 : tests** : `a_niveau_zero_le_texte_est_stable` (deux instants différents, même sortie), `le_niveau_ne_depasse_jamais_le_plafond_du_limiteur` (séquence de référence du kick, comparer au plafond de `Channel::Flash`), `jamais_plus_de_trois_changements_par_seconde` (compter sur 10 s), `flashs_coupes_donne_un_hologramme_stable_pas_eteint`, `l_apparition_dure_au_plus_1_5_s`, `la_teinte_suit_la_palette_et_l_etalonnage`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter ; émissif > 1 pour déclencher le bloom existant, halo = version floue de l'atlas, faisceau = dégradé vertical entre barre et texte, lignes de balayage lentes, apparition par balayage. **Step 4 :** vert ; clippy. **Step 5 : commit** `[X-12] Projecteurs holographiques : glow néon et limiteur`.

### Task 7: Qualité

**Files:**
- Modify: `scenery/config.rs` (`set_quality`), `graffiti/projector.rs`, `graffiti/mod.rs`
- Test: `scenery/graffiti_plan_tests.rs`, `tests/graffiti_projectors.rs`

**Interfaces:**
- Consumes : `SceneryConfig::set_quality(level)` et `FAR_BY_QUALITY`.
- Produces : `GraffitiQuality { holo_far: bool, halo: bool, wall_far: bool }` dérivé du niveau (ordre de retrait de la spec : petits projecteurs lointains, halo, graffitis muraux lointains).

- [ ] **Step 1 : tests** : `chaque_niveau_retire_dans_l_ordre_de_la_spec`, `en_qualite_minimale_seuls_les_proches_restent`, `changer_de_qualite_ne_detruit_aucune_entite`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter. **Step 4 :** vert. **Step 5 : commit** `[X-12] Qualité : graffitis et projecteurs`.

### Task 8: Réglages F4

**Files:**
- Create: `crates/cf-render/src/graffiti_tuning.rs` (comme `tuning.rs`, pour que l'opérateur ne dépende pas du monde), `crates/cf-operator/src/debug_panel/graffiti.rs`
- Modify: `crates/cf-render/src/lib.rs`, `crates/cf-operator/src/debug_panel/mod.rs` (nouvelle section, enregistrée dans `sections()`), `debug_panel/persist.rs`, `crates/cf-operator/src/debug_panel/tests.rs`, le monde (lire le réglage)

**Interfaces:**
- Produces : `GraffitiTuning { wall_enabled: bool, holo_enabled: bool, wall_density: f32, holo_density: f32, forced_word: Option<String> }` (ressource, non sauvegardée dans les looks, réservée à la Préparation comme le reste de F4) ; section « Graffitis » dans le panneau.

- [ ] **Step 1 : tests** : `les_interrupteurs_ecrivent_le_reglage`, `le_forcage_d_un_mot_est_dans_la_liste_des_mots_charges`, `la_densite_est_bornee`, `la_section_est_listee_en_preparation_seulement`.
- [ ] **Step 2 :** lancer, échec. **Step 3 :** implémenter ; la densité multiplie `wall_one_in` et `holo_one_in` (0 = coupé), le forçage affiche toujours le mot choisi dans les projecteurs proches. **Step 4 :** `cargo nextest run -p cf-operator -p cf-render -p cf-world-neon` vert. **Step 5 : commit** `[X-12] Panneau Debug : section Graffitis`.

### Task 9: Documentation et vérifications finales

**Files:**
- Modify: `docs/guide-operateur.md` (section Graffitis et projecteurs : où sont `words.ron` et `placements.ron`, comment ajouter un mot, une police, un placement), `assets/fonts/LICENCES.md`

- [ ] **Step 1 :** `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace` : tout vert.
- [ ] **Step 2 :** vérifier que `scenery.json` n'a pas changé : `git diff --stat crates/cf-world-neon/tests/golden/`.
- [ ] **Step 3 :** dans la description de PR : cocher les critères d'acceptation de la spec avec preuves, et dans « Écarts » dire que la vérification visuelle (tailles, densités, lisibilité depuis la voiture, glow) n'a pas pu être faite par l'agent.
- [ ] **Step 4 : commit** `[X-12] Guide de l'opérateur : graffitis` ; pousser la branche `task/X-12-graffitis` et ouvrir la PR `[X-12] Graffitis et projecteurs holographiques`.
