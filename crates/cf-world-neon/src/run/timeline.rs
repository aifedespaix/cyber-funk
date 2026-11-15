//! La frise de l'ouverture et de la clôture : toutes les durées et bornes d'étapes vivent ici, pour se régler sans
//! toucher à la logique. Pur : aucune dépendance à Bevy.
//!
//! # L'ouverture (`Démarrer`), 30 s : chaque chose est provoquée par la précédente
//!
//! ```text
//!  0 s        2,4 s           ~7 s             9 s          ~15,5 s                  28 s     30 s
//!  ├ la voiture s'allume ─────┤ elle entre dans ┤ le portique ┤ elle passe dessous :   ┤ le quartier ┤ le show
//!  │ hors champ, derrière     │ l'image, frôle  │ sort de la  │ le quartier se lève    │ est en place│ commence
//!  │ la caméra ; les lignes   │ la caméra, file │ route, le   │ autour d'elle, le ciel │ la caméra   │ (premier
//!  │ de la route filent       │ vers l'horizon  │ nom s'allume│ s'ouvre                │ redescend   │ cue)
//! ```
//!
//! La voiture n'« apparaît » pas : elle arrive (plan `opening`). Aucun cue, aucun événement ne part avant 30 s : le
//! spectacle ne s'arme qu'une fois le quartier levé (`RunPhase::show_armed`, `RunPhase::settled`).
//!
//! # La clôture (`Arrêter`), 20 s : le miroir
//!
//! ```text
//!  0 s      3,8 s              7,6 s            9,4 s                  12 s             20 s
//!  ├ la caméra ┤ la voiture met  ┤ ses feux s'éteignent ┤ (elle freine, hors champ)
//!  │ revient   │ les gaz et file │ au point de fuite    │
//!  ├──── le quartier rentre sous terre ────────────────────────────────┤ le ciel et la route retombent ┤ attente
//!  │ la caméra la laisse partir, puis glisse sur la route nue jusqu'à la pose d'attente (plan `closing`)
//! ```

use crate::intro::ramp;

/// Durée de l'ouverture (s).
pub const START_S: f32 = 30.0;
/// La voiture démarre (s après `Démarrer`) : avant, elle s'allume à l'arrêt, hors champ.
pub const LAUNCH_S: f32 = 2.4;
/// Elle a atteint sa vitesse d'ouverture (s).
pub const LAUNCHED_S: f32 = 6.0;
/// Vitesse de l'ouverture (m/s) : assez pour un passage franc devant la caméra, sous l'allure calme du show.
pub const OPENING_MPS: f32 = 22.0;
/// Vitesse à la fin de l'ouverture (m/s) : la voiture arrive lancée dans le show.
pub const OPENING_END_MPS: f32 = 26.0;
/// Le portique sort de la route (s après `Démarrer`), 160 m devant la voiture.
pub const GANTRY_RISE_S: f32 = 8.0;
/// Le nom du DJ commence à s'allumer (s).
pub const GANTRY_START_S: f32 = 9.0;
/// Le portique est rentré sous la route (s) : la voiture et la caméra sont passées depuis longtemps.
pub const GANTRY_END_S: f32 = 22.0;
/// Le quartier commence à se lever (s) : l'instant où la voiture passe sous le portique.
pub const BUILD_START_S: f32 = 15.5;
/// Le quartier est en place (s) : il reste deux secondes de ville complète avant le premier cue.
pub const BUILD_END_S: f32 = 28.0;
/// Durée de la clôture (s).
pub const STOP_S: f32 = 20.0;
/// La voiture commence à s'éteindre, loin devant (s après `Arrêter`).
pub const FADE_START_S: f32 = 7.6;
/// Elle a disparu au point de fuite (s) : elle freine alors, hors champ.
pub const GONE_S: f32 = 9.4;
/// Le quartier est entièrement sous terre (s).
pub const FOLDED_S: f32 = 12.0;
/// Vitesse minimale visée par la voiture qui s'en va (m/s), avant le surplus.
pub const FAREWELL_MIN_MPS: f32 = 30.0;
/// La voiture met les gaz (s après `Arrêter`) : avant, la caméra vient se placer derrière elle.
pub const FAREWELL_START_S: f32 = 3.8;
/// Elle a atteint sa vitesse de départ (s).
pub const FAREWELL_TOP_S: f32 = 6.8;
/// Surplus de vitesse de la voiture qui s'en va (m/s).
pub const FAREWELL_BOOST_MPS: f32 = 16.0;

/// Route dans la scène d'attente : assez pour deviner la scène, sans rien dévoiler.
pub const WAIT_ROAD: f32 = 0.25;
/// Ciel dans la scène d'attente.
pub const WAIT_SKY: f32 = 0.3;

/// Vitesse de croisière d'une voiture seule (m/s) : le plafond du pilote pendant une passation.
pub const CRUISE_MPS: f32 = 16.0;

/// Durée de la montée du volume de la musique (s).
pub const MUSIC_FADE_IN_S: f32 = 4.0;
/// Durée de la descente du volume de la musique (s).
pub const MUSIC_FADE_OUT_S: f32 = 12.0;

/// Le plan que la séquence impose à la caméra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraPlan {
    /// Le montage automatique est libre.
    Free,
    /// Le plan d'attente : au ras de la route, devant la voiture hors champ.
    Waiting,
    /// Le plan-séquence de l'ouverture.
    Opening,
    /// Le plan-séquence de la clôture.
    Closing,
    /// Le plan de passation (les deux voitures).
    Handover,
}

/// Les étapes de la séquence, de 0 (absent) à 1 (en place).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunStages {
    /// Lignes néon de la route.
    pub road: f32,
    /// Phares de la voiture.
    pub lights: f32,
    /// Immeubles qui montent.
    pub build: f32,
    /// Fenêtres allumées, par un front devant la voiture.
    pub windows: f32,
    /// Ciel, lune et planètes.
    pub sky: f32,
    /// Enseigne du DJ, lettre par lettre.
    pub sign: f32,
    /// Immeubles qui rentrent sous terre (clôture, passation).
    pub collapse: f32,
    /// Présence de la voiture : 0 absente, 1 pleinement là (voir `car/sync.rs`).
    pub car_presence: f32,
    /// Volume de la musique de la séquence, 0 à 1.
    pub music: f32,
    /// Le portique : 0 absent, ~0,5 au-dessus de la voiture, 1 replié sous la route.
    pub gantry: f32,
    /// Part de glitch du nom sur le portique, 0 stable à 1 plein glitch (géométrique, coupé en mode calme).
    pub name_glitch: f32,
    /// Le plan imposé à la caméra.
    pub camera: CameraPlan,
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
        collapse: 0.0,
        car_presence: 0.0,
        music: 0.0,
        gantry: 0.0,
        name_glitch: 0.0,
        camera: CameraPlan::Waiting,
    };

    /// Le show joue : tout est en place, la séquence n'a plus d'effet.
    pub const RUNNING: Self = Self {
        road: 1.0,
        lights: 1.0,
        build: 1.0,
        windows: 1.0,
        sky: 1.0,
        sign: 1.0,
        collapse: 0.0,
        car_presence: 1.0,
        music: 1.0,
        gantry: 0.0,
        name_glitch: 0.0,
        camera: CameraPlan::Free,
    };
}

pub(super) fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Les étapes `t_s` secondes après `Démarrer` (voir la frise en tête de module). `reduce_motion` supprime le glitch du
/// nom.
#[must_use]
pub fn starting_stages(t_s: f32, reduce_motion: bool) -> RunStages {
    let glitch = if reduce_motion || t_s < GANTRY_START_S {
        0.0
    } else {
        1.0 - ramp(t_s, GANTRY_START_S + 0.3, GANTRY_START_S + 2.2)
    };
    RunStages {
        // Les lignes filent vers l'horizon avant la voiture : elles l'annoncent.
        road: mix(WAIT_ROAD, 1.0, ramp(t_s, 0.8, 4.8)),
        lights: ramp(t_s, 0.8, 3.0),
        build: ramp(t_s, BUILD_START_S, BUILD_END_S),
        // Les lumières sont celles des tours (shader) : plus de front global, une tour allume ses fenêtres une fois sortie.
        windows: 1.0,
        sky: mix(WAIT_SKY, 1.0, ramp(t_s, BUILD_START_S - 1.0, BUILD_END_S - 2.0)),
        sign: ramp(t_s, GANTRY_START_S + 0.5, GANTRY_START_S + 4.0),
        collapse: 0.0,
        // Hors champ, derrière la caméra : personne ne la voit arriver à sa taille.
        car_presence: ramp(t_s, 0.2, 1.6),
        music: ramp(t_s, 0.0, MUSIC_FADE_IN_S),
        // Absent avant, replié à la fin ; la voiture passe dessous au milieu (~0,5).
        gantry: if t_s < GANTRY_RISE_S {
            0.0
        } else {
            ramp(t_s, GANTRY_RISE_S, GANTRY_END_S)
        },
        name_glitch: glitch,
        camera: CameraPlan::Opening,
    }
}

/// Vitesse imposée à la voiture (m/s) `t_s` secondes après `Démarrer` : à l'arrêt le temps de s'allumer, un départ
/// franc, puis une montée douce vers l'allure du show.
#[must_use]
pub fn starting_speed(t_s: f32) -> f32 {
    OPENING_MPS * ramp(t_s, LAUNCH_S, LAUNCHED_S) + (OPENING_END_MPS - OPENING_MPS) * ramp(t_s, BUILD_START_S, START_S)
}

/// Secondes écoulées depuis que le quartier a commencé à se lever (négatif avant) : la grue de l'ouverture s'y cale.
#[must_use]
pub fn city_seconds(t_s: f32) -> f32 {
    t_s - BUILD_START_S
}

/// Les étapes `t_s` secondes après `Arrêter` (voir la frise en tête de module).
#[must_use]
pub fn stopping_stages(t_s: f32) -> RunStages {
    // Une fois tous les immeubles rentrés ils ne sont plus visibles : on les rend à l'état d'attente.
    let hidden = t_s >= FOLDED_S;
    RunStages {
        road: mix(1.0, WAIT_ROAD, ramp(t_s, FOLDED_S, STOP_S - 1.0)),
        // Les phares suivent la présence de la voiture (`car/sync.rs`) : ils s'éteignent avec elle, au loin.
        lights: 1.0,
        build: if hidden { 0.0 } else { 1.0 },
        windows: 1.0,
        sky: mix(1.0, WAIT_SKY, ramp(t_s, GONE_S + 1.0, STOP_S - 2.0)),
        sign: 1.0 - ramp(t_s, 0.0, 1.5),
        collapse: if hidden { 0.0 } else { ramp(t_s, 1.0, FOLDED_S) },
        // Elle s'éteint au point de fuite : lumières, couleur, puis elle rapetisse comme un objet qui s'éloigne.
        car_presence: 1.0 - ramp(t_s, FADE_START_S, GONE_S),
        music: 1.0 - ramp(t_s, 0.0, MUSIC_FADE_OUT_S),
        gantry: 0.0,
        name_glitch: 0.0,
        camera: CameraPlan::Closing,
    }
}

/// Vitesse imposée à la voiture (m/s) `t_s` secondes après `Arrêter`, partie de `from_mps` : elle met les gaz et file
/// vers l'horizon ; une fois disparue ([`GONE_S`]), elle s'arrête, hors champ.
#[must_use]
pub fn stopping_speed(t_s: f32, from_mps: f32) -> f32 {
    if t_s >= GONE_S {
        return 0.0;
    }
    let top = from_mps.max(FAREWELL_MIN_MPS) + FAREWELL_BOOST_MPS;
    mix(from_mps, top, ramp(t_s, FAREWELL_START_S, FAREWELL_TOP_S))
}
