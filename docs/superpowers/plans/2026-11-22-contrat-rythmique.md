# Contrat rythmique (R1) : plan d'implémentation

> **Pour les agents :** utiliser `superpowers:subagent-driven-development` (recommandé) ou `superpowers:executing-plans`.
> Les étapes sont des cases à cocher.

**Objectif :** une seule porte d'entrée du rythme pour les effets visuels, en trois couches (Mouvement, Accents,
Énergie) et une confiance de grille unique (`grid_trust`) qui pondère le seul Mouvement.

**Architecture :** `cf-render/src/audio/routing` publie déjà `FilteredSignals` (cibles, `beat_phase`, `bar_phase`,
`kick_wave`). On y ajoute `grid_trust` et deux aides de lecture. Les ~40 « ponts » (fichiers qui convertissent
`AudioFeatures` en structures propres à un module : `TrafficBeat`, `BeatView` du cinéma…) migrent vers cette porte, module
par module. Un test « cliquet » interdit toute nouvelle lecture directe et ne laisse que la liste des fichiers pas encore
migrés, qui ne fait que rétrécir.

**Stack :** Rust, Bevy 0.20, `cf-render`, `cf-world-neon`, `cf-cinema`, `proptest`.

**Spec :** `docs/superpowers/specs/2026-11-22-contrat-rythmique-design.md`

## Contraintes globales

- Règles 🔴 de `CLAUDE.md` : pas d'`unwrap`/`expect`/`panic!` hors tests ; pas d'allocation par image dans les systèmes
  chauds ; fichiers de 400 lignes au plus ; tout flash reste derrière le limiteur (`cf-show`).
- `cf-analysis` et `cf-show` ne dépendent pas de Bevy : `grid_trust` se calcule dans `cf-render`, pas dans `cf-analysis`.
- Les champs d'`AudioFeatures` ne sont pas supprimés (règle de C1) : seul change qui les lit.
- Un commit qui compile par étape ; `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` et `cargo nextest run --workspace` verts avant chaque push.
- Commentaires en français, identifiants en anglais. Commits sur `main` (règle de `prompt.md`).

## Focus de relecture (cas que le design implique sans que les tests le disent)

- Perte de signal : `live == false` pendant un fondu de `grid_trust` : la confiance retombe à 0, l'image ne se fige pas.
- Changement de source de tempo (Audio → Clock) : `grid_trust` monte sans saut visible ; pas de saut de phase côté
  Mouvement.
- `bpm == None` ou non fini : le Mouvement ne produit pas de NaN (repli du moteur : 120 BPM).
- Image très longue (`dt` énorme après un gel) : `grid_trust` reste dans [0, 1] et ne dépend pas de `dt`.
- Tempo douteux au micro (`confidence` basse) avec un kick réel : l'accent part quand même (couche Accents).

---

### Tâche 1 : `grid_trust` et le contrat de lecture

**Fichiers :**
- Créer : `crates/cf-render/src/audio/routing/trust.rs`
- Modifier : `crates/cf-render/src/audio/routing/mod.rs` (déclarer et ré-exporter), `routing/signals.rs`,
  `routing/engine.rs`, `routing/tests.rs` (ou `routing/trust.rs` en module de tests)

**Interfaces :**
- Produit : `pub fn trust_target(source: TempoSource, confidence: f32, live: bool) -> f32` (dans [0, 1]) ;
  `pub struct GridTrust { value: f32 }` avec `GridTrust::new() -> Self`, `step(&mut self, dt: f32, target: f32) -> f32`
  (renvoie la valeur lissée) ; `FilteredSignals.grid_trust: f32` (champ public, 0 par défaut) ;
  `FilteredSignals::movement(&self, amplitude: f32) -> f32` = `amplitude * grid_trust` ;
  `KickWave::pulse(&self, tau_s: f32) -> f32` = `exp(-âge_du_dernier_kick / tau_s)`, 0 si aucun kick.
- Consomme : `cf_analysis::TempoSource`, `FilteredSignals`, `RoutingEngine::update`.

Valeurs de départ (à régler avec la sonde de R2, à regrouper dans une constante nommée) : montée `τ = 2,0 s`, descente
`τ = 0,8 s`. Table de `trust_target` : `Clock` et `Link` → 1,0 ; `File` → `confidence.max(0.8)` ; `Audio` et `Tap` →
`confidence` ; `live == false` → 0,0.

- [ ] **Étape 1 : tests (rouges)** dans `routing/tests.rs` :
  - `trust_target_clock_et_link_valent_un`, `trust_target_file_au_moins_08`, `trust_target_audio_suit_la_confiance`,
    `trust_target_sans_signal_vaut_zero` (assertions sur les valeurs ci-dessus, bornées à [0, 1], confiance non finie → 0).
  - `grid_trust_meme_reponse_a_30_60_144_images_par_seconde` (échelon 0 → 1 pendant 3 s : écart < 1 % entre les trois
    cadences, comme les filtres de C1) ; `grid_trust_descend_plus_vite_quil_ne_monte` ;
    `grid_trust_reste_dans_zero_un_meme_avec_dt_enorme` (proptest : `dt` dans [0, 1e6], cible dans [0, 1]).
  - `movement_est_nul_sans_confiance` : `FilteredSignals::default().movement(1.0) == 0.0` ; avec `grid_trust = 1.0` :
    renvoie l'amplitude.
  - `kick_wave_pulse_vaut_un_au_kick_et_zero_sans_kick`.
- [ ] **Étape 2 : lancer** `cargo test -p cf-render routing` : les nouveaux tests échouent (symboles absents).
- [ ] **Étape 3 : implémenter** les signatures ci-dessus. Lissage exponentiel exact (`1 - exp(-dt/τ)`), `dt` borné comme
  dans `RoutingEngine::update` (0 à 0,25 s). `RoutingEngine` porte un `GridTrust` et le met à jour dans `update` à partir de
  `features.tempo_source`, `features.confidence`, `features.live`, puis écrit `out.grid_trust`.
- [ ] **Étape 4 : lancer** `cargo test -p cf-render routing` : tout est vert.
- [ ] **Étape 5 : commit** `R1 : confiance de grille (grid_trust) et aides de lecture du contrat rythmique`.

### Tâche 2 : le cliquet (test de non-régression)

**Fichiers :**
- Créer : `crates/cf-render/tests/rhythm_contract.rs`, `crates/cf-render/tests/rhythm_allowlist.txt`
- Modèle de test existant à imiter : `crates/cf-render/tests/no_frustum_culling_guard.rs`

**Interfaces :**
- Produit : un test `rhythm_reads_only_through_the_contract` ; un fichier `rhythm_allowlist.txt` (un chemin relatif par
  ligne, les fichiers pas encore migrés).

Règle testée : parmi les fichiers `.rs` de `cf-world-neon/src`, `cf-cinema/src`, `cf-render/src` (hors `audio/`, hors
tests, hors `examples/`), seuls ceux de la liste peuvent lire les champs de grille ou d'accent d'`AudioFeatures`
(`kick`, `pulse`, `count`, `phase`, `bar`, `phrase`, `beat`, `onset`, `hat`, `transient`) sur une valeur de type
`AudioFeatures`. Le test échoue (a) pour un fichier hors liste qui en lit, et (b) pour une entrée de la liste qui n'en lit
plus (la liste doit rétrécir).

- [ ] **Étape 1 : écrire le test** (balayage de texte, sans dépendance nouvelle sauf si `regex` est déjà dans l'espace de
  travail : sinon, balayage à la main des motifs `.kick`, `.pulse`, `.phase`, etc. en excluant `.count(` et les lignes de
  commentaire `//`). Le message d'échec nomme le fichier et la règle.
- [ ] **Étape 2 : générer** `rhythm_allowlist.txt` à partir de l'état actuel (tous les fichiers qui violent la règle
  aujourd'hui), pour que le test passe tel quel.
- [ ] **Étape 3 : prouver que le test mord** : ajouter temporairement `let _ = features.kick;` dans un fichier hors liste,
  vérifier l'échec avec un message clair, annuler.
- [ ] **Étape 4 : lancer** `cargo test -p cf-render --test rhythm_contract` : vert.
- [ ] **Étape 5 : commit** `R1 : test cliquet du contrat rythmique et liste des fichiers à migrer`.

### Tâche 3 : migrer le cinéma et le trafic

**Fichiers :**
- Modifier : `crates/cf-world-neon/src/cinema.rs` (construit `BeatView`), `crates/cf-world-neon/src/traffic/frame.rs`
  (`TrafficBeat::from`), `traffic/systems.rs`, et les fichiers de `rhythm_allowlist.txt` du cinéma et du trafic ;
  `crates/cf-cinema/src/types.rs` si `BeatView` doit gagner un champ.
- Modifier : `crates/cf-render/tests/rhythm_allowlist.txt` (retirer les fichiers migrés).

**Interfaces :**
- Consomme : `FilteredSignals` (`beat_phase`, `bar_phase`, `grid_trust`, `kick_wave`, `movement`, `KickWave::pulse`).
- Produit : `TrafficBeat::from_signals(&FilteredSignals, &AudioFeatures) -> TrafficBeat` (les champs d'Énergie : `bpm`,
  `section`, `intensity`, `live` viennent encore d'`AudioFeatures`) ; l'équivalent pour `BeatView` du cinéma.

Classement à appliquer à chaque lecture (voir le design) : un mouvement continu (`beat`, `phase`, `bar`, `phrase`) passe
par `FilteredSignals` et `movement(…)` ; un accent (`kick`, `pulse`, `onset`, `hat`, `transient`) passe par `KickWave` ou la
matrice ; l'énergie (`intensity`, `section`, `bass`, `loud`) peut rester sur `AudioFeatures`.

- [ ] **Étape 1 : tests (rouges)** pour chaque pont migré : à `grid_trust = 0`, la part Mouvement est nulle et l'accent d'un
  kick reste non nul ; à `grid_trust = 1`, la part Mouvement vaut l'amplitude. Ex. `traffic_beat_sans_confiance_garde_laccent`.
- [ ] **Étape 2 : lancer** `cargo test -p cf-world-neon -p cf-cinema` : échecs attendus.
- [ ] **Étape 3 : migrer** les ponts du cinéma et du trafic selon le classement. Chaque cas où un accent doit rester lié au
  son sans passer par la matrice reçoit un commentaire qui dit pourquoi.
- [ ] **Étape 4 : mettre à jour** la liste du cliquet (retirer ce qui est migré) ; `cargo test -p cf-render --test
  rhythm_contract` vert.
- [ ] **Étape 5 : captures et sonde** : une capture avant/après d'un plan de caméra sur la démo, dans `docs/captures/R1/`
  (dire dans le journal ce qui n'a pas pu être vérifié à l'image).
- [ ] **Étape 6 : commit** `R1 : cinéma et trafic lisent le rythme par le contrat`.

### Tâche 4 : migrer landmarks, enseignes, voiture, pilote, faisceaux, cascades

**Fichiers :** `cf-world-neon/src/{landmarks,signs,car,pilot,djsign,beams,stunts}/` (fichiers de la liste du cliquet) et
`crates/cf-render/tests/rhythm_allowlist.txt`.

**Interfaces :** mêmes que la tâche 3, aucune nouvelle.

- [ ] **Étape 1 : tests (rouges)** : au moins un test par module migré qui vérifie la règle de confiance sur la formule
  d'entrée de ce module (Mouvement à 0 sans confiance, Accent conservé), sur le modèle de la tâche 3.
- [ ] **Étape 2 : migrer** les ponts, un module par commit, en gardant le cliquet vert à chaque commit.
- [ ] **Étape 3 : commits** `R1 : <module> lit le rythme par le contrat` (un par module).

### Tâche 5 : migrer le reste et fermer le cliquet

**Fichiers :** tout fichier encore listé dans `rhythm_allowlist.txt` (façades, météo, ciel, route, atmosphère, graffitis,
chute, intro, météores, `cf-render/src/{world,palette,warmup,streaming,run}`) ; `rhythm_allowlist.txt`.

- [ ] **Étape 1 : migrer** chaque fichier restant, ou le justifier : s'il lit un champ d'accent pour une raison qui ne peut
  pas passer par la matrice (ex. un test de démarrage), l'entrée de la liste porte un commentaire de justification
  (`# raison`) et le test l'accepte.
- [ ] **Étape 2 : lancer** `cargo test -p cf-render --test rhythm_contract` : la liste ne contient plus que des entrées
  justifiées.
- [ ] **Étape 3 : commit** `R1 : le contrat rythmique est la seule porte d'entrée`.

### Tâche 6 : documentation, journal, vérification

**Fichiers :** créer `docs/rythme.md` (le contrat en une page : trois couches, `grid_trust`, qui lit quoi, comment ajouter
un effet) ; modifier `docs/worldbuilding/CHANTIERS.md` (ligne R1 → ✅, journal, ce qui reste à juger sur le matériel) ;
modifier `docs/superpowers/specs/2026-11-22-contrat-rythmique-design.md` (corrections ci-dessous).

- [ ] **Étape 1 : écrire** `docs/rythme.md` et le journal de `CHANTIERS.md`. Y noter : les constantes de `grid_trust` sont
  des valeurs de départ, la sensation de danse se juge sur le matériel cible, le test de synchronisation arrive avec R2.
- [ ] **Étape 2 : corriger le design** : la migration porte sur ~40 ponts, pas sur ~90 lectures éparses ; le routage par
  défaut ne gagne pas de routes de phase (le Mouvement lit `FilteredSignals.beat_phase`/`bar_phase`, déjà latence
  compensés).
- [ ] **Étape 3 : vérification complète** : `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`, `cargo nextest run --workspace` ; coller la fin de chaque sortie dans le journal.
- [ ] **Étape 4 : commit et push** sur `main` (`git pull --rebase origin main` d'abord).

---

## Auto-relecture

- Couverture du design : trois couches (tâches 3 à 5), règle de confiance (tâche 1), porte d'entrée et cliquet
  (tâches 2 à 5), vérification et documentation (tâche 6). « Routes de phase par défaut » est écartée volontairement : le
  design est corrigé en tâche 6.
- Cohérence des types : `grid_trust`, `trust_target`, `GridTrust::step`, `movement`, `KickWave::pulse` ont les mêmes noms
  partout.
- Non couvert : le test de synchronisation musique/image (R2), le réglage fin des constantes (matériel cible).
