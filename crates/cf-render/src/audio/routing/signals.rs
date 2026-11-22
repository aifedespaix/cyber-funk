//! Les signaux filtrés de l'image : ce que les systèmes de rendu et les shaders lisent, à la place de `AudioFeatures`.

use bevy::prelude::*;
use cf_analysis::SPECTRUM_BANDS;

use super::types::Target;

/// Âge (s) attribué à un kick qui n'a pas eu lieu : assez grand pour que toute onde soit éteinte.
pub const KICK_NEVER_S: f32 = 60.0;

/// Constante de temps (s) de l'impulsion d'accent d'un kick (`KickWave::pulse`). L'ancien `pulse` d'`AudioFeatures` était
/// linéaire (1 → 0 en 0,22 s, `PULSE_DECAY` = 4,5 /s) ; une exponentielle de même aire a pour constante 1/(2·4,5).
pub const KICK_PULSE_TAU_S: f32 = 0.11;

/// Les quatre derniers kicks sous forme d'âges (s), du plus récent au plus ancien : l'« onde de kick » commune de
/// P1 §0. Des âges plutôt que des horodatages absolus : un `f32` perd la milliseconde au bout d'une heure d'horloge,
/// pas un âge borné à [`KICK_NEVER_S`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KickWave {
    ages: [f32; 4],
}

impl Default for KickWave {
    fn default() -> Self {
        Self {
            ages: [KICK_NEVER_S; 4],
        }
    }
}

impl KickWave {
    /// Un kick vient d'avoir lieu : il devient le plus récent.
    pub fn push(&mut self) {
        self.ages.rotate_right(1);
        self.ages[0] = 0.0;
    }

    /// Le temps passe de `dt` secondes.
    pub fn age(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        for a in &mut self.ages {
            *a = (*a + dt).min(KICK_NEVER_S);
        }
    }

    /// Les quatre âges, prêts à devenir un `vec4` d'uniforme.
    #[must_use]
    pub const fn ages(&self) -> [f32; 4] {
        self.ages
    }

    /// Impulsion du dernier kick : 1 à l'instant du kick, puis `exp(-âge/τ)` ; 0 si aucun kick (ou oublié).
    ///
    /// C'est l'accent du contrat rythmique : il vient du son réel (message `Kick`, latence compensée), jamais de la grille.
    #[must_use]
    pub fn pulse(&self, tau_s: f32) -> f32 {
        let age = self.ages[0];
        if age >= KICK_NEVER_S || !tau_s.is_finite() || tau_s <= 0.0 {
            return 0.0;
        }
        (-age / tau_s).exp()
    }

    /// Rayon (m) du front de l'onde `i` à la vitesse `speed` (m/s) ; négatif si elle n'existe pas.
    #[must_use]
    pub fn radius(&self, i: usize, speed: f32) -> f32 {
        match self.ages.get(i) {
            Some(&age) if age < KICK_NEVER_S => age * speed,
            _ => -1.0,
        }
    }
}

/// Les signaux filtrés. Tout est de taille fixe : la ressource est écrasée sur place à chaque image.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FilteredSignals {
    values: [f32; Target::COUNT],
    /// Spectre en 32 bandes logarithmiques : One-Euro puis crête qui retombe en 200 ms, 0..1.
    pub spectrum: [f32; SPECTRUM_BANDS],
    /// Phase du temps vue à l'écran (latence compensée), 0..1.
    pub beat_phase: f32,
    /// Phase de la mesure vue à l'écran (latence compensée), 0..1.
    pub bar_phase: f32,
    /// Onde de kick.
    pub kick_wave: KickWave,
    /// Confiance dans la grille rythmique, 0..1 (voir [`GridTrust`](super::trust::GridTrust)). Pondère le Mouvement.
    pub grid_trust: f32,
    /// Temps musical continu, en temps (4 par mesure), calé sur la phase de la grille et compensé de la latence : il ne
    /// recule jamais. C'est l'horloge du Mouvement ; à pondérer par [`movement`](Self::movement).
    pub grid_beat: f64,
    /// Horloge de danse, en temps : continue, ne recule jamais, calée sur les mesures de la grille quand elle est fiable
    /// (voir [`dance_beat`](Self::dance_beat)). Écrite par le moteur.
    pub dance: f64,
    /// Transitoires aigus du son réel, 0 à 1 (accent, sans grille).
    pub transient_accent: f32,
    /// Attaque du son réel, 0 à 1 (accent, sans grille).
    pub onset_accent: f32,
    /// Charleston du son réel, 0 à 1 (accent, sans grille).
    pub hat_accent: f32,
    /// Temps continu piloté par les kicks (`count` plus une montée douce) : l'horloge des motifs pas-à-pas de Sillage.
    /// Il ne dépend pas de la grille.
    pub kick_beat: f64,
    /// Nombre de kicks vus depuis le démarrage. Ne retombe jamais à zéro : on compte les kicks par différence.
    pub kick_count: u32,
    /// Vrai pendant l'image où un kick vient d'être vu (accent).
    pub kick_now: bool,
}

impl Default for FilteredSignals {
    fn default() -> Self {
        Self {
            values: [0.0; Target::COUNT],
            spectrum: [0.0; SPECTRUM_BANDS],
            beat_phase: 0.0,
            bar_phase: 0.0,
            kick_wave: KickWave::default(),
            grid_trust: 0.0,
            grid_beat: 0.0,
            dance: 0.0,
            transient_accent: 0.0,
            onset_accent: 0.0,
            hat_accent: 0.0,
            kick_beat: 0.0,
            kick_count: 0,
            kick_now: false,
        }
    }
}

impl FilteredSignals {
    /// Valeur d'une cible, dans les bornes de sa route.
    #[must_use]
    pub const fn get(&self, target: Target) -> f32 {
        self.values[target.index()]
    }

    /// Des signaux au repos dont l'impulsion d'accent vaut `pulse` (0 à 1) : pour les démos et les tests qui n'ont pas de
    /// moteur de routage et veulent pourtant un kick à un niveau donné.
    #[must_use]
    pub fn at_pulse(pulse: f32) -> Self {
        let mut s = Self::default();
        if pulse > 0.0 {
            s.kick_wave.push();
            s.kick_wave.age(-KICK_PULSE_TAU_S * pulse.min(1.0).ln());
        }
        s
    }

    /// L'impulsion d'accent du dernier kick (1 → 0), avec la constante de temps commune [`KICK_PULSE_TAU_S`].
    #[must_use]
    pub fn kick_pulse(&self) -> f32 {
        self.kick_wave.pulse(KICK_PULSE_TAU_S)
    }

    /// L'horloge de danse, en temps : la grille quand elle est fiable (avec hystérésis, voir le moteur), sinon le nombre de
    /// kicks, sans saut ni recul au changement de source. Une mesure commence quand `floor(dance_beat / 4)` change ; sur
    /// la grille, ce changement tombe sur le temps 1.
    #[must_use]
    pub const fn dance_beat(&self) -> f64 {
        self.dance
    }

    /// Début d'image : efface ce qui ne dure qu'une image (`kick_now`).
    pub fn begin_frame(&mut self) {
        self.kick_now = false;
    }

    /// Un kick vient d'être vu : l'accent part (onde, compteur, drapeau d'image).
    pub fn register_kick(&mut self) {
        self.kick_wave.push();
        self.kick_count = self.kick_count.wrapping_add(1);
        self.kick_now = true;
    }

    /// Une amplitude de **mouvement** (calée sur la phase), pondérée par la confiance dans la grille : nulle quand la
    /// grille est douteuse, pleine quand elle est sûre. Les accents n'y passent pas.
    #[must_use]
    pub fn movement(&self, amplitude: f32) -> f32 {
        amplitude * self.grid_trust
    }

    /// Les mêmes signaux, la cible `target` fixée à la main. Pour les démos, les captures et les tests : en show, seul le
    /// moteur de routage écrit les cibles (bornes, pente, limiteur).
    #[must_use]
    pub const fn with(mut self, target: Target, value: f32) -> Self {
        self.values[target.index()] = value;
        self
    }

    pub(super) fn set(&mut self, target: Target, value: f32) {
        self.values[target.index()] = value;
    }

    pub(super) fn clear_targets(&mut self) {
        self.values = [0.0; Target::COUNT];
    }
}

/// Phase vue à l'écran : `φ_vis = fract(φ + (L_audio + L_rendu)/T_b)` (P2 §2.2). Une latence négative ou non finie
/// est ignorée.
#[must_use]
pub fn visual_phase(phase: f32, latency_s: f32, period_s: f32) -> f32 {
    let shift = if latency_s.is_finite() && period_s > 0.0 {
        latency_s.max(0.0) / period_s
    } else {
        0.0
    };
    let p = if phase.is_finite() { phase } else { 0.0 };
    (p + shift).rem_euclid(1.0)
}
