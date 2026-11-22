//! Confiance de grille : un seul nombre de 0 à 1 qui dit à quel point la grille rythmique (phase, mesure) est fiable.
//!
//! Il pondère uniquement la couche **Mouvement** du contrat rythmique (`docs/rythme.md`) : balancement, tangage,
//! respiration, tout ce qui se cale sur la phase. Les accents (kick, caisse claire) et l'énergie restent justes sans
//! grille, donc ils ne l'utilisent pas. Une grille fausse est pire qu'une absence de grille : l'image danserait à
//! contretemps.

use cf_analysis::TempoSource;

/// Constante de temps de la montée (s) : la confiance se gagne lentement, pour ne pas s'enthousiasmer sur trois kicks.
pub const RISE_TAU_S: f32 = 2.0;
/// Constante de temps de la descente (s) : elle se perd un peu plus vite, car une grille fausse se voit tout de suite.
pub const FALL_TAU_S: f32 = 0.8;
/// Plancher de la grille d'une piste de démo analysée hors ligne : tempo et phase exacts.
const FILE_FLOOR: f32 = 0.8;
/// Plus grand `dt` pris en compte (s), comme dans `RoutingEngine::update` : après un gel, l'image ne saute pas.
const MAX_DT_S: f32 = 0.25;

/// La confiance visée d'après la source du tempo, la confiance de l'arbitre et la présence de signal.
///
/// Horloge MIDI et Link : 1 (c'est le DJ qui donne la grille). Grille de démo : au moins 0,8. Audio et tap : la
/// confiance de l'arbitre. Sans signal ou avec une confiance non finie : 0.
#[must_use]
pub fn trust_target(source: TempoSource, confidence: f32, live: bool) -> f32 {
    if !live || !confidence.is_finite() {
        return 0.0;
    }
    let c = confidence.clamp(0.0, 1.0);
    match source {
        TempoSource::Clock | TempoSource::Link => 1.0,
        TempoSource::File => c.max(FILE_FLOOR),
        TempoSource::Audio | TempoSource::Tap => c,
    }
}

/// La confiance lissée : montée lente, descente plus rapide, indépendante de la cadence d'images.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GridTrust {
    value: f32,
}

impl GridTrust {
    /// Confiance nulle : tant qu'aucune grille n'est établie, le mouvement ne dépend pas d'elle.
    #[must_use]
    pub const fn new() -> Self {
        Self { value: 0.0 }
    }

    /// Fixe la valeur (tests).
    #[cfg(test)]
    pub(super) fn set(&mut self, value: f32) {
        self.value = value;
    }

    /// Avance de `dt` secondes vers `target` ; renvoie la valeur lissée, toujours dans [0, 1].
    pub fn step(&mut self, dt: f32, target: f32) -> f32 {
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT_S) } else { 0.0 };
        let target = if target.is_finite() {
            target.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let tau = if target > self.value { RISE_TAU_S } else { FALL_TAU_S };
        let k = 1.0 - (-dt / tau).exp();
        self.value = (self.value + (target - self.value) * k).clamp(0.0, 1.0);
        self.value
    }
}

#[cfg(test)]
mod tests {
    use cf_analysis::TempoSource;

    use super::*;
    use crate::audio::routing::{FilteredSignals, KickWave};

    #[test]
    fn trust_target_clock_et_link_valent_un() {
        assert_eq!(trust_target(TempoSource::Clock, 0.0, true), 1.0);
        assert_eq!(trust_target(TempoSource::Link, 0.1, true), 1.0);
    }

    #[test]
    fn trust_target_file_au_moins_08() {
        assert_eq!(trust_target(TempoSource::File, 0.2, true), 0.8);
        assert_eq!(trust_target(TempoSource::File, 0.95, true), 0.95);
    }

    #[test]
    fn trust_target_audio_et_tap_suivent_la_confiance() {
        assert!((trust_target(TempoSource::Audio, 0.4, true) - 0.4).abs() < 1e-6);
        assert!((trust_target(TempoSource::Tap, 0.7, true) - 0.7).abs() < 1e-6);
        assert_eq!(trust_target(TempoSource::Audio, 3.0, true), 1.0, "borné à 1");
    }

    #[test]
    fn trust_target_sans_signal_ou_confiance_non_finie_vaut_zero() {
        assert_eq!(trust_target(TempoSource::Clock, 1.0, false), 0.0);
        assert_eq!(trust_target(TempoSource::Audio, f32::NAN, true), 0.0);
        assert_eq!(trust_target(TempoSource::File, f32::INFINITY, true), 0.0);
    }

    fn settle(fps: f32, seconds: f32, target: f32, start: f32) -> f32 {
        let mut t = GridTrust::new();
        t.set(start);
        let dt = 1.0 / fps;
        let mut v = start;
        for _ in 0..(seconds * fps).round() as usize {
            v = t.step(dt, target);
        }
        v
    }

    #[test]
    fn grid_trust_meme_reponse_a_30_60_144_images_par_seconde() {
        let a = settle(30.0, 3.0, 1.0, 0.0);
        let b = settle(60.0, 3.0, 1.0, 0.0);
        let c = settle(144.0, 3.0, 1.0, 0.0);
        assert!((a - b).abs() < 0.01, "{a} {b}");
        assert!((b - c).abs() < 0.01, "{b} {c}");
        assert!(a > 0.5 && a < 0.99, "la montée n'est ni nulle ni instantanée : {a}");
    }

    #[test]
    fn grid_trust_descend_plus_vite_quil_ne_monte() {
        let up = settle(60.0, 1.0, 1.0, 0.0);
        let down = 1.0 - settle(60.0, 1.0, 0.0, 1.0);
        assert!(down > up, "descente {down} contre montée {up}");
    }

    #[test]
    fn grid_trust_reste_dans_zero_un_meme_avec_dt_enorme() {
        for dt in [0.0, 1e-9, 0.016, 0.25, 10.0, 1e6, f32::NAN, f32::INFINITY, -3.0] {
            for target in [0.0, 0.3, 1.0] {
                let mut t = GridTrust::new();
                let v = t.step(dt, target);
                assert!((0.0..=1.0).contains(&v), "dt {dt} cible {target} -> {v}");
            }
        }
    }

    #[test]
    fn movement_est_nul_sans_confiance() {
        assert_eq!(FilteredSignals::default().movement(1.0), 0.0);
        let mut s = FilteredSignals::default();
        s.grid_trust = 1.0;
        assert_eq!(s.movement(0.7), 0.7);
    }

    #[test]
    fn kick_wave_pulse_vaut_un_au_kick_et_zero_sans_kick() {
        let mut w = KickWave::default();
        assert_eq!(w.pulse(0.1), 0.0, "aucun kick");
        w.push();
        assert_eq!(w.pulse(0.1), 1.0, "à l'instant du kick");
        w.age(0.1);
        assert!((w.pulse(0.1) - (-1.0_f32).exp()).abs() < 1e-5);
        w.age(1000.0);
        assert_eq!(w.pulse(0.1), 0.0, "kick oublié");
    }
}
