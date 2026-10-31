//! Le déroulé de la pré-intro (`show/pre-intro-timeline.ts`) : des étapes qui se chevauchent le long d'une progression
//! `t` de 0 à 1. Pur : aucune dépendance à Bevy.

/// Chaque étape va de 0 (absente) à 1 (en place).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntroStages {
    /// Les lignes néon de la route se dessinent depuis la voiture jusqu'à l'horizon.
    pub road: f32,
    /// Les phares de la voiture.
    pub lights: f32,
    /// Les immeubles montent (révélation holographique).
    pub build: f32,
    /// Fenêtres et néons des façades, allumés par vagues sur les kicks.
    pub windows: f32,
    /// Ciel, lune et planètes en fondu.
    pub sky: f32,
    /// L'enseigne du DJ s'allume, lettre par lettre.
    pub sign: f32,
    /// La voiture prend sa vitesse de pré-intro (le moteur monte).
    pub drive: f32,
}

impl IntroStages {
    /// Tout en place : la pré-intro n'a plus aucun effet.
    pub const DONE: Self = Self {
        road: 1.0,
        lights: 1.0,
        build: 1.0,
        windows: 1.0,
        sky: 1.0,
        sign: 1.0,
        drive: 1.0,
    };
}

/// Bornes `[début, fin]` de chaque étape sur `t` (`STAGE_RANGES`).
const ROAD: [f32; 2] = [0.0, 0.2];
const LIGHTS: [f32; 2] = [0.05, 0.14];
const BUILD: [f32; 2] = [0.14, 0.58];
const WINDOWS: [f32; 2] = [0.34, 0.8];
const SKY: [f32; 2] = [0.42, 0.68];
const SIGN: [f32; 2] = [0.6, 0.9];
const DRIVE: [f32; 2] = [0.1, 0.5];

/// Durée de base de la pré-intro (s) : l'énergie la raccourcit jusqu'à moitié.
pub const INTRO_SECONDS: f32 = 45.0;

/// Avance de `t` par seconde après la sortie du niveau 0 : ce qui manque s'achève en ~3 s.
pub const FINISH_RATE: f32 = 0.4;

/// Part de la durée que l'énergie maximale retranche (`1 + 0,9 × énergie` de Sillage).
const ENERGY_SPEEDUP: f32 = 0.9;

/// Marche lissée de 0 à 1 entre `a` et `b` (`ramp` de `dj-sign-anim.ts`).
#[must_use]
pub fn ramp(v: f32, a: f32, b: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Les étapes à la progression `t`.
#[must_use]
pub fn stages_at(t: f32) -> IntroStages {
    IntroStages {
        road: ramp(t, ROAD[0], ROAD[1]),
        lights: ramp(t, LIGHTS[0], LIGHTS[1]),
        build: ramp(t, BUILD[0], BUILD[1]),
        windows: ramp(t, WINDOWS[0], WINDOWS[1]),
        sky: ramp(t, SKY[0], SKY[1]),
        sign: ramp(t, SIGN[0], SIGN[1]),
        drive: ramp(t, DRIVE[0], DRIVE[1]),
    }
}

/// Avance de `t` pour une image : plus l'énergie est haute, plus vite le monde se construit.
#[must_use]
pub fn advance(t: f32, dt: f32, energy: f32, finishing: bool) -> f32 {
    if t >= 1.0 {
        return 1.0;
    }
    let rate = if finishing {
        FINISH_RATE
    } else {
        (1.0 + energy.clamp(0.0, 1.0) * ENERGY_SPEEDUP) / INTRO_SECONDS
    };
    (t + dt * rate).min(1.0)
}

/// Vitesse maximale (m/s) de la voiture pendant le niveau 0 : un filet au début, un petit train à la fin.
#[must_use]
pub fn speed_cap(drive: f32) -> f32 {
    3.0 + 13.0 * drive
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_absent_at_zero_and_in_place_at_one() {
        let start = stages_at(0.0);
        assert!(start.build.abs() < f32::EPSILON && start.sky.abs() < f32::EPSILON && start.sign.abs() < f32::EPSILON);
        assert_eq!(stages_at(1.0), IntroStages::DONE);
    }

    #[test]
    fn stages_overlap_in_sillage_order() {
        // Au tiers de la pré-intro : la route est tracée, les immeubles montent, le ciel n'est pas encore là.
        let s = stages_at(0.3);
        assert!((s.road - 1.0).abs() < f32::EPSILON);
        assert!(s.build > 0.0 && s.build < 1.0);
        assert!(s.sky.abs() < f32::EPSILON && s.sign.abs() < f32::EPSILON);
    }

    #[test]
    fn energy_shortens_the_intro_by_up_to_half() {
        let dt = 0.1;
        let steps = |energy: f32| {
            let (mut t, mut n) = (0.0, 0_u32);
            while t < 1.0 {
                t = advance(t, dt, energy, false);
                n += 1;
            }
            n
        };
        let calm = steps(0.0);
        let loud = steps(1.0);
        assert!((calm as f32 * dt - 45.0).abs() < 0.2, "{calm}");
        assert!(loud < calm * 6 / 10 && loud > calm * 5 / 10, "{loud} contre {calm}");
    }

    #[test]
    fn finishing_takes_about_two_and_a_half_seconds_from_scratch() {
        let (mut t, mut elapsed) = (0.0_f32, 0.0_f32);
        while t < 1.0 {
            t = advance(t, 0.05, 0.0, true);
            elapsed += 0.05;
        }
        assert!((elapsed - 2.5).abs() < 0.1, "{elapsed}");
    }

    #[test]
    fn speed_cap_grows_with_the_drive_stage() {
        assert!((speed_cap(0.0) - 3.0).abs() < f32::EPSILON);
        assert!((speed_cap(1.0) - 16.0).abs() < f32::EPSILON);
    }
}
