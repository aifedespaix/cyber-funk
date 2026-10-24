//! La logique pure de la qualité adaptative : portage de la classe `Quality` de `engine/quality.ts`.
//!
//! Aucun type Bevy hors le `derive(Resource)`, aucune horloge lue en dur : on lui donne les durées d'image (et,
//! quand le pilote les fournit, les temps GPU) et elle répond par le nouveau niveau quand il change. C'est ce qui
//! permet de la tester avec des mesures injectées, sans GPU.
//!
//! Écarts avec Sillage (voir la PR) : la mesure GPU, et une fenêtre de mesure bornée aussi en durée.

use bevy::prelude::Resource;

/// Échelle de la résolution de rendu pour chaque niveau de qualité (0 = plein régime).
pub const PIXEL_SCALE: [f32; 4] = [1.0, 0.9, 0.8, 0.7];
/// Dernier niveau (le plus dégradé).
pub const MAX_LEVEL: usize = PIXEL_SCALE.len() - 1;

/// Démarrage : on ne juge rien pendant les premières secondes (chargement, caches froids).
const WARMUP_SECONDS: f64 = 4.0;
/// Images par fenêtre de mesure.
const WINDOW_FRAMES: u32 = 120;
/// Durée maximale d'une fenêtre de mesure (s). Sillage ne comptait que des images : à 20 i/s, 120 images font 6 s, trop
/// long pour réagir. Une fenêtre se ferme donc aussi après cette durée, pourvu qu'elle ait `MIN_WINDOW_FRAMES` images.
const WINDOW_SECONDS: f64 = 2.0;
/// Images minimales d'une fenêtre fermée par la durée (en dessous, la moyenne ne dit rien).
const MIN_WINDOW_FRAMES: u32 = 8;
/// Temps moyen par image au-delà duquel on dégrade : ~30 images/s. Un flou franc ne vaut que si ça saccade vraiment.
const SLOW_FRAME: f64 = 1.0 / 30.0;
/// Pour remonter, on estime le temps d'image qu'on aurait au cran du dessus (le coût suit à peu près le nombre de
/// pixels) : sous ~38 images/s estimées, c'est tenable, et bien au-dessus du seuil de dégradation (hystérésis).
const RAISE_FRAME: f64 = 1.0 / 38.0;
/// Une image plus longue que ça (~24 i/s) est un accroc visible, même si la moyenne reste bonne (ramasse-miettes,
/// compilation de shader).
const HITCH_FRAME: f64 = 1.0 / 24.0;
/// Part des images d'une fenêtre qui peuvent être des accrocs avant qu'on dégrade.
const HITCH_SHARE: f64 = 0.1;
/// Attente (s) après un changement avant d'en faire un autre vers le bas.
const COOLDOWN_SECONDS: f64 = 4.0;
/// Fenêtres de mesure d'affilée où la remontée est estimée tenable, avant de remonter d'un cran (~4 s).
const GOOD_WINDOWS: u32 = 2;
/// Attente (s) avant de remonter après un changement ; doublée à chaque rechute, jusqu'à `MAX_PATIENCE`.
const BASE_PATIENCE: f64 = 5.0;
const MAX_PATIENCE: f64 = 40.0;
/// Une rechute qui survient moins de ce temps après une remontée prouve que la remontée était trop optimiste.
const RELAPSE_SECONDS: f64 = 25.0;
/// Une image plus longue que ça est une pause (fenêtre masquée, chargement), pas une mesure.
const STALL: f64 = 0.25;
/// Immobilité (s) avant de retrouver la pleine qualité.
const SETTLE_SECONDS: f64 = 1.2;

/// Surveille la fluidité et règle le niveau de qualité : sous ~30 images/s, un cran de moins (résolution de rendu,
/// puis bloom, puis particules). Elle remonte d'elle-même dès que le cran du dessus est estimé tenable (~4 s), sans
/// attendre qu'on ralentisse. Pour ne pas osciller, la remontée attend un peu, et son attente double à chaque
/// rechute : si le plein régime ne tient pas, on le laisse tranquille.
///
/// Les durées sont en secondes ; l'horloge interne n'avance qu'avec les images mesurées.
#[derive(Resource, Debug, Clone)]
pub struct Quality {
    level: usize,
    elapsed: f64,
    since_change: f64,
    total: f64,
    frames: u32,
    hitches: u32,
    gpu_total: f64,
    gpu_frames: u32,
    good: u32,
    idle_for: f64,
    patience: f64,
    last_raise: f64,
}

impl Default for Quality {
    fn default() -> Self {
        Self::new()
    }
}

impl Quality {
    /// Plein régime, en phase de démarrage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: 0,
            elapsed: 0.0,
            since_change: COOLDOWN_SECONDS,
            total: 0.0,
            frames: 0,
            hitches: 0,
            gpu_total: 0.0,
            gpu_frames: 0,
            good: 0,
            idle_for: 0.0,
            patience: BASE_PATIENCE,
            last_raise: f64::NEG_INFINITY,
        }
    }

    /// Niveau courant, de 0 (plein) à [`MAX_LEVEL`].
    #[must_use]
    pub fn level(&self) -> usize {
        self.level
    }

    /// Patience courante avant une remontée (s) : double à chaque rechute.
    #[must_use]
    pub fn patience(&self) -> f64 {
        self.patience
    }

    fn reset_window(&mut self) {
        self.total = 0.0;
        self.frames = 0;
        self.hitches = 0;
        self.gpu_total = 0.0;
        self.gpu_frames = 0;
    }

    /// Impose un niveau (mode manuel de l'opérateur) et repart d'une fenêtre vierge.
    pub fn force_level(&mut self, level: usize) {
        self.level = level.min(MAX_LEVEL);
        self.since_change = 0.0;
        self.good = 0;
        self.reset_window();
    }

    /// À l'arrêt ou presque (`idle`), on retrouve tout de suite la netteté : une image quasi fixe ne coûte presque rien,
    /// et rester flou alors qu'on ne bouge plus n'a aucun intérêt. Dès que ça repart, la mesure habituelle reprend et
    /// redégrade si la machine ne suit pas. Renvoie `Some(0)` quand la qualité vient de revenir au maximum.
    pub fn settle(&mut self, idle: bool, dt: f64) -> Option<usize> {
        if !idle {
            self.idle_for = 0.0;
            return None;
        }
        self.idle_for += dt;
        if self.idle_for < SETTLE_SECONDS || self.level == 0 {
            return None;
        }
        self.level = 0;
        self.since_change = 0.0;
        self.good = 0;
        self.reset_window();
        Some(0)
    }

    /// Mesure de l'image qui vient de s'achever : durée réelle `frame_s` et, si le pilote fournit des timestamps GPU,
    /// temps GPU `gpu_s` (somme des passes de premier niveau). Renvoie le nouveau niveau s'il a changé.
    ///
    /// Le temps GPU sert à deux choses que la durée d'image ne dit pas. Sous vsync la durée reste collée à 16,7 ms
    /// tant que le GPU suit : seule la mesure GPU dit de combien on est en dessous, donc si le cran du dessus est
    /// tenable. Et un GPU saturé (> `SLOW_FRAME`) dégrade même si la durée d'image paraît bonne.
    pub fn sample(&mut self, frame_s: f64, gpu_s: Option<f64>) -> Option<usize> {
        if !frame_s.is_finite() || frame_s <= 0.0 {
            return None;
        }
        if frame_s > STALL {
            self.reset_window();
            self.good = 0;
            return None;
        }
        self.elapsed += frame_s;
        self.since_change += frame_s;
        if self.elapsed < WARMUP_SECONDS {
            return None;
        }

        self.total += frame_s;
        self.frames += 1;
        if frame_s > HITCH_FRAME {
            self.hitches += 1;
        }
        if let Some(gpu) = gpu_s.filter(|g| g.is_finite() && *g >= 0.0) {
            self.gpu_total += gpu;
            self.gpu_frames += 1;
        }
        let full = self.frames >= WINDOW_FRAMES;
        let long = self.total >= WINDOW_SECONDS && self.frames >= MIN_WINDOW_FRAMES;
        if !(full || long) {
            return None;
        }
        self.close_window()
    }

    fn close_window(&mut self) -> Option<usize> {
        let average = self.total / f64::from(self.frames);
        let gpu_average = (self.gpu_frames > 0).then(|| self.gpu_total / f64::from(self.gpu_frames));
        // La moyenne cache les à-coups : on compte aussi les images vraiment longues.
        let hitchy = f64::from(self.hitches) / f64::from(self.frames) > HITCH_SHARE;
        self.reset_window();

        if average > SLOW_FRAME || hitchy || gpu_average.is_some_and(|gpu| gpu > SLOW_FRAME) {
            self.good = 0;
            if self.level >= MAX_LEVEL || self.since_change < COOLDOWN_SECONDS {
                return None;
            }
            // Rechute juste après une remontée : on attendra deux fois plus longtemps la prochaine.
            if self.elapsed - self.last_raise < RELAPSE_SECONDS {
                self.patience = (self.patience * 2.0).min(MAX_PATIENCE);
            }
            self.level += 1;
            self.since_change = 0.0;
            return Some(self.level);
        }

        // La machine détecte seule qu'elle peut mieux faire : elle n'attend pas d'être au repos. Coût de référence :
        // le temps GPU quand on l'a (borné par la durée d'image : le GPU ne peut pas coûter plus que l'image entière).
        let cost = gpu_average.map_or(average, |gpu| gpu.min(average));
        let ratio = if self.level > 0 {
            f64::from(PIXEL_SCALE[self.level - 1] / PIXEL_SCALE[self.level])
        } else {
            1.0
        };
        self.good = if !hitchy && cost * ratio * ratio < RAISE_FRAME {
            self.good + 1
        } else {
            0
        };
        if self.level > 0 && self.good >= GOOD_WINDOWS && self.since_change >= self.patience {
            self.level -= 1;
            self.since_change = 0.0;
            self.good = 0;
            self.last_raise = self.elapsed;
            return Some(self.level);
        }
        None
    }
}
