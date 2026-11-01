# Façades vivantes : implémentation

> **Pour les agents :** sous-compétence requise : `superpowers:subagent-driven-development` (recommandé) ou
> `superpowers:executing-plans`. Les étapes utilisent des cases `- [ ]`.

**Objectif :** donner aux immeubles un tempérament propre, des formes et mouvements de lumière variés, des fenêtres à vie
propre et un suivi de la voiture, sans casser le limiteur photosensible ni le budget d'image.

**Architecture :** le tempérament d'un immeuble est calculé côté Rust (fonction pure de la rangée et de la graine), rangé
dans les 8 bits libres du `MeshTag`, et la table des tempéraments est envoyée au shader en uniforme : une seule source de
vérité, testée en Rust. Les nouvelles formes et mouvements vivent dans deux nouveaux fichiers WESL concaténés à
`facade.wesl` (comme `field.wesl`) pour rester sous 400 lignes. La traîne de la voiture est un tampon circulaire de
16 positions envoyé en uniforme.

**Stack :** Rust, Bevy (`cf-world-neon`), WESL, `cf-render` (palette, qualité), `cargo nextest`, `insta` si utile.

**Spec :** `docs/superpowers/specs/2026-11-01-facades-lumieres-design.md`

**Branche :** `task/X-09-facades-vivantes` (suite de X-08 ; l'identifiant est une hypothèse à confirmer avec
l'orchestrateur). Ne pas toucher à `docs/TASKS.md`.

## Contraintes globales

- Pas de `unwrap`/`expect`/`panic!` hors tests ; pas d'allocation par image ; aucun shader compilé pendant un show (règles
  2, 4, 8). Pas de variantes de shader : repli par branche uniforme.
- Toute variation rapide passe par le limiteur : au plus 3 changements par seconde, jamais de front plus raide que
  `MIN_FADE`, rien si `CurrentShowFx.flashes` est faux ou en mode respiration (comme `TextureFlash` dans
  `facade/systems.rs`).
- Le temps du shader boucle toutes les 2 400 s (`TIME_WRAP_S`) : toute période de mouvement nouvelle doit diviser 2 400 s
  (ou être sans saut au rebouclage).
- Fichiers de plus de 400 lignes à découper (règle 11) ; commentaires en français, identifiants en anglais.
- Avant chaque commit : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo nextest run --workspace`, avec `CARGO_INCREMENTAL=0`. Itérer avec `-p cf-world-neon`.
- Les fichiers `assets/looks/niveau-*.ron` existants doivent rester valides.

## Points de vigilance (testés dans les tâches indiquées)

- Voiture à l'arrêt : la traîne ne divise pas par zéro et n'accumule pas des positions identiques (Task 5).
- Recalage de l'origine flottante : les positions de la traîne suivent le recalage sans saut (Task 5).
- Changement de thème en plein show (`PaletteMorph`) : les camaïeux suivent sans à-coup (Task 2).
- Arches (tag posé à la main, tempérament 0) : rendu inchangé (Task 1).
- Immeubles lointains (`far`) : aucune forme ne scintille quand les fenêtres deviennent sous-pixel (Task 3).
- Flashs coupés par le spectacle ou mode respiration : plus aucun scintillement, la ville reste lisible (Task 4).
- Rebouclage du temps toutes les 2 400 s : aucun saut visible des nouveaux mouvements (Task 4).
- Qualité basse : traîne, écrans qui défilent et fenêtres à vie propre coupés, le reste inchangé (Task 7).

---

### Task 1: Le tempérament d'un immeuble (Rust)

**Fichiers :**
- Créer : `crates/cf-world-neon/src/facade/temperament.rs`
- Modifier : `crates/cf-world-neon/src/scenery/layout.rs` (`Part` : champ `temper: u8`, `Part::tag` ; lignes ~150-175),
  `crates/cf-world-neon/src/facade/mod.rs` (déclarer le module et réexporter)
- Test : dans `temperament.rs` (`#[cfg(test)]`) et `crates/cf-world-neon/tests/scenery_golden.rs`

**Interfaces :**
- Produit : `pub const TEMPER_COUNT: usize = 8;`
  `pub struct Temperament { pub hue_span_deg: f32, pub lit_bias: f32, pub amber_share: f32, pub ice_share: f32, pub shape: LightShape, pub motion: MotionKind, pub spark_rate: f32 }`
  `pub enum LightShape { Panes, Bands, Columns, Sign, Screen, FloorTint }`
  `pub enum MotionKind { Steady, WaveUp, WaveDown, Drift, Sweep, Pulse }`
  `pub const TEMPERAMENTS: [Temperament; TEMPER_COUNT];` (index 0 = « bureau stable » : reproduit le rendu actuel)
  `pub fn temper_of(row_index: i64, seed: u16) -> u8` (0..TEMPER_COUNT, fonction pure).
- Produit : `Part::tag()` met `temper` dans les bits 24..26 ; les bits 0..23 sont inchangés.

- [ ] **Étape 1 : tests qui échouent** : `temper_is_deterministic` (même entrée, même sortie sur 10 000 appels),
  `neighbours_differ` (sur 200 rangées consécutives, au moins 5 tempéraments distincts apparaissent et deux voisins sont
  identiques moins d'une fois sur 3), `distribution_is_calm_by_default` (le tempérament 0 et les autres « calmes »
  couvrent au moins 60 % des 10 000 premières rangées, conformément à l'ambiance calme et dense décidée dans la spec),
  `tag_keeps_legacy_fields` (pour toute pièce, `tag & 0xFF_FFFF` vaut l'ancienne valeur), `arch_tag_has_temper_zero`.
- [ ] **Étape 2 : lancer** `cargo nextest run -p cf-world-neon temperament` ; attendu : échec (symboles absents).
- [ ] **Étape 3 : implémenter** `temperament.rs` et le champ `Part::temper` (hachage de `row_index` et `seed` pour
  `temper_of`). Les valeurs de la table : hue_span 0..40°, lit_bias −0,15..+0,2 ; le tempérament 0 a `Steady`, `Panes`,
  `hue_span_deg = 0`, `lit_bias = 0`, parts ambre/glace actuelles (30 %/10 %), `spark_rate = 0`.
- [ ] **Étape 4 : relancer** les tests ; attendu : PASS. Puis `cargo nextest run -p cf-world-neon --test scenery_golden` ;
  si les références changent, vérifier que seul le tag diffère et le documenter dans la PR.
- [ ] **Étape 5 : commit** `[X-09] Tempérament des immeubles dans le tag`.

### Task 2: Camaïeux et densité par tempérament (shader + uniformes)

**Fichiers :**
- Modifier : `crates/cf-world-neon/src/facade/material.rs` (`FacadeParams` : `tempers: [Vec4; 2 * TEMPER_COUNT]`),
  `crates/cf-world-neon/src/facade/systems.rs` (`build_params` remplit la table), `assets/shaders/cf-world-neon/facade.wesl`
  (struct `Facade` dans le même ordre de champs ; lecture du tempérament via `get_tag(...) >> 24`, voir `vertex`, lignes
  ~103-145 pour l'acheminement de `in.info`)
- Test : `crates/cf-world-neon/src/facade/systems.rs` (tests existants de `build_params`)

**Interfaces :**
- Consomme : `TEMPERAMENTS`, `Temperament`, `temper_of` (Task 1).
- Produit : `fn hue_shifted(color: Vec3, deg: f32) -> Vec3` en WGSL (décalage de teinte, voisins de la teinte de tour) ;
  `FacadeParams.tempers[2*i]` = (hue_span_deg, lit_bias, amber_share, ice_share), `[2*i+1]` = (shape, motion, spark_rate, 0).

- [ ] **Étape 1 : test qui échoue** : `build_params_exposes_temperament_table` (la table envoyée au shader reflète
  `TEMPERAMENTS` entrée par entrée) et `temperament_zero_is_neutral` (les valeurs du tempérament 0 sont celles qui
  reproduisent le rendu d'avant : hue_span 0, lit_bias 0, ambre 0,3, glace 0,1).
- [ ] **Étape 2 : lancer** `cargo nextest run -p cf-world-neon facade` ; attendu : échec.
- [ ] **Étape 3 : implémenter** l'uniforme, la lecture du tempérament dans le fragment, puis : (a) la teinte de chaque
  fenêtre = teinte de tour décalée dans ±`hue_span_deg/2` selon son hachage ; (b) `lit_chance += lit_bias` ; (c) parts
  ambre/glace lues de la table au lieu des constantes 0,7 et 0,9 (lignes ~249-251). La dérivation reste à partir de
  `cycle`/rôles de palette, donc suit `PaletteMorph`.
- [ ] **Étape 4 : relancer** les tests ; puis capture avant/après sur la scène de référence (voir tâche 7 pour la commande)
  et vérifier que le tempérament 0 est identique à l'avant. Vérifier à la main un changement de thème en plein défilement
  (pas d'à-coup).
- [ ] **Étape 5 : commit** `[X-09] Camaïeux et densité par tempérament`.

### Task 3: Formes de lumière

**Fichiers :**
- Créer : `assets/shaders/cf-world-neon/facade_shapes.wesl` (concaténé à `FACADE_SOURCE` dans `material.rs`, comme
  `field.wesl`)
- Modifier : `facade.wesl` (appel selon `LightShape`), `material.rs` (concaténation)
- Test : `crates/cf-world-neon/tests/scenery_golden.rs` (ou nouvelle capture dans `docs/captures/`)

**Interfaces :**
- Consomme : la forme du tempérament (Task 2).
- Produit : `fn shape_mask(shape: f32, cell: vec2<f32>, f: vec2<f32>, seed: f32, fade_out: f32) -> f32` en WGSL : masque 0..1
  de la zone allumée d'une cellule (`Panes` = cadre actuel).

- [ ] **Étape 1 : test qui échoue** : un test Rust d'intégration qui rend une rangée de référence avec chacune des 6 formes
  forcées et vérifie que les images diffèrent deux à deux (écart moyen > seuil) et que, avec `fade_out = 1`, toutes les
  formes donnent la même teinte moyenne (± tolérance) : aucune forme ne scintille au loin.
- [ ] **Étape 2 : lancer** ; attendu : échec.
- [ ] **Étape 3 : implémenter** les six masques : fenêtres pleines, bandeaux horizontaux (une travée sur deux d'un étage),
  colonnes verticales, enseigne de façade (bloc de 2×2 travées), écran qui défile (bande qui avance avec `time`, période
  qui divise 2 400 s), étage teinté entier. Chacune se dégrade en teinte moyenne par `fade_out` ; le rythme des étages
  reste visible de loin.
- [ ] **Étape 4 : relancer** ; attendu : PASS. Capturer une vue de ville et vérifier à l'œil qu'au moins 4 formes
  coexistent (critère de la spec).
- [ ] **Étape 5 : commit** `[X-09] Formes de lumière des façades`.

### Task 4: Mouvements lisibles et garde-fous de flash

**Fichiers :**
- Créer : `assets/shaders/cf-world-neon/facade_motion.wesl`
- Modifier : `facade.wesl`, `facade/systems.rs` (nouvelle voie limitée `MotionFlash` à côté de `TextureFlash`),
  `facade/material.rs` (uniforme `motion: Vec4` : x phase de balayage limitée, y part de scintillement autorisée,
  z, w réservés)
- Test : `crates/cf-world-neon/src/facade/systems.rs` et `crates/cf-world-neon/tests/flash_ceiling.rs`

**Interfaces :**
- Consomme : `MotionKind` (Task 1), `BeamPulse` (`beams/safety.rs`).
- Produit : `fn motion_gain(kind: f32, h: f32, u: f32, seed: f32, time: f32, beat: f32, sweep: f32) -> f32` en WGSL (facteur
  multiplicatif de lumière d'un immeuble) ; côté Rust `MotionFlash::update(dt_s, now_s, policy, drive) -> MotionDrive`.

- [ ] **Étape 1 : tests qui échouent** : `motion_flash_caps_rate` (une entrée à 10 Hz ne donne jamais plus de 3 départs par
  seconde), `motion_flash_off_without_flashes_policy` (`policy = Some((false, _))` ou respiration : sortie nulle),
  `motion_periods_divide_wrap` (toutes les périodes déclarées par `MotionKind` divisent 2 400 s), et dans
  `flash_ceiling.rs` un cas façades : sur une séquence de référence, la luminance moyenne de l'écran ne dépasse pas les
  seuils du limiteur.
- [ ] **Étape 2 : lancer** ; attendu : échec.
- [ ] **Étape 3 : implémenter** vagues montantes/descendantes (sur `beat`), bandes qui glissent le long de la rue,
  balayage de projecteur ; la part « rapide » est multipliée par la sortie de `MotionFlash`.
- [ ] **Étape 4 : relancer** ; attendu : PASS. Vérifier à l'œil qu'à t = 2 399,9 s puis 0 s il n'y a pas de saut (imposer
  `FacadeLook.clock_s`).
- [ ] **Étape 5 : commit** `[X-09] Mouvements de lumière limités`.

### Task 5: Suivi de la voiture (traîne)

**Fichiers :**
- Créer : `crates/cf-world-neon/src/facade/trail.rs`
- Modifier : `facade/mod.rs` (ressource `CarTrail`, système), `material.rs` (`trail: [Vec4; 16]`, `car_tint: Vec4`),
  `facade/systems.rs`, `facade.wesl` (+ `facade_motion.wesl`)
- Test : `trail.rs` et `crates/cf-world-neon/tests/` (un test sans allocation dans le style de `traffic_no_alloc.rs`)

**Interfaces :**
- Consomme : la position absolue de la voiture (composant `CarState`, `car/rig.rs`; `previous: Option<DVec3>`),
  `FloatingOrigin::origin()`.
- Produit : `pub struct CarTrail { /* tampon circulaire de 16 */ }` avec
  `pub fn record(&mut self, position: DVec3, now_s: f64)` (n'ajoute un point que si la voiture a avancé d'au moins 6 m) et
  `pub fn params(&self, origin: DVec3, now_s: f64) -> [Vec4; 16]` (xyz repère rendu, w = intensité restante 0..1).

- [ ] **Étape 1 : tests qui échouent** : `record_skips_when_stopped` (voiture immobile : un seul point, pas de NaN),
  `ring_wraps_and_keeps_latest_16`, `intensity_decays_with_age` (w décroît puis tombe à 0 après la durée de traîne
  choisie, 6 s), `params_follow_origin_rebase` (déplacer l'origine de 1 000 m ne change pas les positions absolues),
  `trail_update_does_not_allocate`.
- [ ] **Étape 2 : lancer** ; attendu : échec.
- [ ] **Étape 3 : implémenter** `CarTrail`, son système (`CfSet::Present`, avant `update_material`) et, dans le shader, la
  lueur d'une façade = max sur les 16 points de `w × exp(−(distance au point / 40 m)²)`, teintée de `car_tint` (couleur
  de la voiture, `CarState.glow`) à ~25 %. Aucune lumière dynamique, aucun état par immeuble.
- [ ] **Étape 4 : relancer** ; attendu : PASS. Rouler un tour de route courbe (exemple `neon_world`) et vérifier que la
  traîne suit les virages.
- [ ] **Étape 5 : commit** `[X-09] Traîne lumineuse de la voiture sur les façades`.

### Task 6: Fenêtres à vie propre

**Fichiers :**
- Modifier : `facade.wesl`, `facade_motion.wesl`, `facade/systems.rs` (la part autorisée passe par `MotionFlash`)
- Test : `crates/cf-world-neon/src/facade/systems.rs`, `crates/cf-world-neon/tests/scenery_golden.rs`

**Interfaces :**
- Consomme : `Temperament.spark_rate` (Task 1), `MotionDrive` (Task 4).
- Produit : `fn window_life(cell: vec2<f32>, seed: f32, time: f32, spark_rate: f32, allow: f32) -> f32` en WGSL : trois
  comportements fixes par cellule (télé bleutée qui papillote, extinction/rallumage lent, néon qui grésille), tirés du
  hachage de la cellule et jamais de l'image.

- [ ] **Étape 1 : tests qui échouent** : deux images rendues à t et t + 2 400 s sont identiques (comportement déterministe,
  critère « même motif ») ; part de cellules concernées ≤ `spark_rate` ; `allow = 0` : rendu identique à sans cette
  fonction.
- [ ] **Étape 2 : lancer** ; attendu : échec.
- [ ] **Étape 3 : implémenter** `window_life` et l'insérer après le calcul de `lit` (ligne ~236 de `facade.wesl`) ; les
  scintillements sont plafonnés à 3 changements/s et aucun ne s'applique aux immeubles `far`.
- [ ] **Étape 4 : relancer** ; attendu : PASS. Vérification visuelle : une fenêtre qui grésille grésille de la même façon
  quand on revient la voir.
- [ ] **Étape 5 : commit** `[X-09] Fenêtres à vie propre`.

### Task 7: Réglages, qualité basse, mesures, captures

**Fichiers :**
- Modifier : `facade/mod.rs` (`FacadeLook` : `car_follow: f32`, `window_life: f32`, par défaut 1,0), le chargement des looks
  (`assets/looks/niveau-*.ron` : retrouver par `grep -rn FacadeLook crates` comment ils atteignent `FacadeLook`, ajouter
  les champs avec `#[serde(default)]`), `scenery/config.rs` (`set_quality`), `crates/cf-operator` (deux curseurs, dans
  l'onglet qui règle déjà les façades ou le monde), `docs/guide-operateur.md`
- Test : tests de désérialisation des looks existants ; test de `set_quality`

**Interfaces :**
- Consomme : `QualityMode` (`cf-render/src/quality/systems.rs`), `SceneryConfig::set_quality(level: usize) -> bool`.
- Produit : `FacadeLook.car_follow`, `FacadeLook.window_life` ; en qualité basse, `build_params` force `trail` à zéro,
  `motion.y` (scintillement) à zéro et désactive les écrans qui défilent.

- [ ] **Étape 1 : tests qui échouent** : `legacy_looks_still_parse` (tous les `assets/looks/niveau-*.ron` se chargent sans
  les nouveaux champs), `low_quality_disables_costly_effects`, `look_defaults_are_one`.
- [ ] **Étape 2 : lancer** ; attendu : échec.
- [ ] **Étape 3 : implémenter** les champs, le repli, les curseurs et la doc opérateur.
- [ ] **Étape 4 : mesure** : sur la même séquence (exemple `neon_world`, qualité haute), relever le temps GPU par image
  avant/après (stash ou branche de départ) et noter les deux chiffres dans la PR ; sans écran (cloud), dire dans la PR
  qu'ils n'ont pas pu être mesurés. Déposer les captures dans `docs/captures/X-09/`.
- [ ] **Étape 5 : passe complète** : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo nextest run --workspace` ; tout vert.
- [ ] **Étape 6 : commit** `[X-09] Réglages, repli qualité basse et captures` ; PR `[X-09] Façades vivantes` avec les critères
  d'acceptation de la spec cochés un par un et leur preuve.
