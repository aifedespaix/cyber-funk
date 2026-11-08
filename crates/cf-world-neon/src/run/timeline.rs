//! La frise du démarrage et de l'arrêt : toutes les durées et bornes d'étapes vivent ici, pour se régler sans toucher
//! à la logique. Pur : aucune dépendance à Bevy.

use crate::intro::{ramp, speed_cap};

/// Durée de la séquence de démarrage (s).
pub const START_S: f32 = 18.0;
/// Durée de la séquence d'arrêt (s).
pub const STOP_S: f32 = 9.0;

/// Route dans la scène d'attente : assez pour deviner la scène, sans rien dévoiler.
pub const WAIT_ROAD: f32 = 0.25;
/// Ciel dans la scène d'attente.
pub const WAIT_SKY: f32 = 0.3;

/// Décélération maximale à l'arrêt (m/s²) : confort visuel.
pub const BRAKE_MPS2: f32 = 4.0;
/// Vitesse de croisière à laquelle l'arrêt commence (m/s), égale au plafond de fin de démarrage.
pub const CRUISE_MPS: f32 = 16.0;

/// Durée de la montée du volume de la musique (s).
pub const MUSIC_FADE_IN_S: f32 = 4.0;
/// Durée de la descente du volume de la musique (s).
pub const MUSIC_FADE_OUT_S: f32 = 8.0;

/// Les étapes de la séquence, de 0 (absent) à 1 (en place).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunStages {
    /// Lignes néon de la route.
    pub road: f32,
    /// Phares de la voiture.
    pub lights: f32,
    /// Immeubles qui montent (façon Matrix).
    pub build: f32,
    /// Fenêtres allumées, par un front devant la voiture.
    pub windows: f32,
    /// Ciel, lune et planètes.
    pub sky: f32,
    /// Enseigne du DJ, lettre par lettre.
    pub sign: f32,
    /// Vitesse de la voiture, en part du plafond de croisière.
    pub drive: f32,
    /// Immeubles qui rétrécissent et se désynchronisent (arrêt).
    pub collapse: f32,
    /// Présence de la voiture : 0 éteinte et repliée, 1 pleine.
    pub car_presence: f32,
    /// Volume de la musique de la séquence, 0 à 1.
    pub music: f32,
    /// La caméra reste sur le plan d'attente.
    pub camera_waiting: bool,
}

impl RunStages {
    /// La scène d'attente.
    pub const WAITING: Self = Self {
        road: WAIT_ROAD,
        lights: 0.0,
        build: 0.0,
        windows: 0.0,
        sky: WAIT_SKY,
        sign: 0.0,
        drive: 0.0,
        collapse: 0.0,
        car_presence: 0.0,
        music: 0.0,
        camera_waiting: true,
    };

    /// Le show joue : tout est en place, la séquence n'a plus d'effet.
    pub const RUNNING: Self = Self {
        road: 1.0,
        lights: 1.0,
        build: 1.0,
        windows: 1.0,
        sky: 1.0,
        sign: 1.0,
        drive: 1.0,
        collapse: 0.0,
        car_presence: 1.0,
        music: 1.0,
        camera_waiting: false,
    };
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Les étapes `t_s` secondes après `Démarrer`.
#[must_use]
pub fn starting_stages(t_s: f32) -> RunStages {
    RunStages {
        road: mix(WAIT_ROAD, 1.0, ramp(t_s, 0.5, 5.0)),
        lights: ramp(t_s, 1.0, 3.0),
        build: ramp(t_s, 2.5, 11.0),
        windows: ramp(t_s, 7.0, 14.0),
        sky: mix(WAIT_SKY, 1.0, ramp(t_s, 7.0, 14.0)),
        sign: ramp(t_s, 13.0, 17.5),
        drive: ramp(t_s, 1.5, 9.0),
        collapse: 0.0,
        car_presence: ramp(t_s, 0.0, 2.5),
        music: ramp(t_s, 0.0, MUSIC_FADE_IN_S),
        camera_waiting: t_s < 2.5,
    }
}

/// Les étapes `t_s` secondes après `Arrêter`.
#[must_use]
pub fn stopping_stages(t_s: f32) -> RunStages {
    // Une fois tous les immeubles rentrés ils ne sont plus visibles : on les rend à l'état d'attente.
    let hidden = t_s >= 6.5;
    RunStages {
        road: mix(1.0, WAIT_ROAD, ramp(t_s, 6.0, 9.0)),
        lights: 1.0 - ramp(t_s, 5.0, 6.5),
        build: if hidden { 0.0 } else { 1.0 },
        windows: 1.0 - ramp(t_s, 0.0, 2.0),
        sky: mix(1.0, WAIT_SKY, ramp(t_s, 6.0, 9.0)),
        sign: 1.0 - ramp(t_s, 0.0, 1.5),
        drive: (1.0 - t_s * BRAKE_MPS2 / CRUISE_MPS).max(0.0),
        collapse: if hidden { 0.0 } else { ramp(t_s, 1.0, 6.5) },
        car_presence: 1.0 - ramp(t_s, 5.0, 6.5),
        music: 1.0 - ramp(t_s, 0.0, MUSIC_FADE_OUT_S),
        camera_waiting: t_s >= 6.0,
    }
}

/// Plafond de vitesse (m/s) au démarrage : la voiture prend de la vitesse avec l'étape `drive`.
#[must_use]
pub fn starting_cap(stages: &RunStages) -> f32 {
    speed_cap(stages.drive)
}

/// Décélération (m/s²) d'un arrêt qui part de `from_mps` : confortable, mais jamais plus longue que ~4,5 s.
#[must_use]
pub fn brake_rate(from_mps: f32) -> f32 {
    BRAKE_MPS2.max(from_mps / 4.5)
}

/// Plafond de vitesse (m/s) à l'arrêt : décélération constante depuis la vitesse réelle `from_mps`, jusqu'à l'arrêt
/// complet. Partir de la vitesse réelle évite un coup de frein brutal quand la voiture roule plus vite que la croisière.
#[must_use]
pub fn stopping_cap(t_s: f32, from_mps: f32) -> f32 {
    (from_mps - brake_rate(from_mps) * t_s).max(0.0)
}
