//! Les signaux filtrés de l'image : ce que les systèmes de rendu et les shaders lisent, à la place de `AudioFeatures`.

use bevy::prelude::*;
use cf_analysis::SPECTRUM_BANDS;

use super::types::Target;

/// Âge (s) attribué à un kick qui n'a pas eu lieu : assez grand pour que toute onde soit éteinte.
pub const KICK_NEVER_S: f32 = 60.0;

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
    /// Spectre en 32 bandes logarithmiques : One-Euro puis crête (τ_d = 200 ms), 0..1.
    pub spectrum: [f32; SPECTRUM_BANDS],
    /// Phase du temps vue à l'écran (latence compensée), 0..1.
    pub beat_phase: f32,
    /// Phase de la mesure vue à l'écran (latence compensée), 0..1.
    pub bar_phase: f32,
    /// Onde de kick.
    pub kick_wave: KickWave,
}

impl Default for FilteredSignals {
    fn default() -> Self {
        Self {
            values: [0.0; Target::COUNT],
            spectrum: [0.0; SPECTRUM_BANDS],
            beat_phase: 0.0,
            bar_phase: 0.0,
            kick_wave: KickWave::default(),
        }
    }
}

impl FilteredSignals {
    /// Valeur d'une cible, dans les bornes de sa route.
    #[must_use]
    pub const fn get(&self, target: Target) -> f32 {
        self.values[target.index()]
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
