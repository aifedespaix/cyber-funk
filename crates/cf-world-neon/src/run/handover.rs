//! La frise du changement de DJ. Pur : aucune dépendance à Bevy.
//!
//! ```text
//!  0 s      ~9 s            ~9-10 s            +8 s            +3-5 s        +4 s      +2,5 s     +17 s
//!  ├ ralentit, les tours buguent plus fort, s'enfoncent ┤
//!  │ l'ancienne roule seule sur la route nue ───────────────────────────────────────────┤
//!  │        caméra devant elle (≈5 s)                    │
//!  │                          ├ la nouvelle se synchronise loin derrière, rattrape ┤
//!  │                          │ l'ancienne se range sur le côté ┤
//!  │                                                    ├ côte à côte ┤├ l'ancienne se désynchronise ┤
//!  │                                                                                  ├ portique ┤├ quartier ┤
//! ```
//!
//! Les instants dépendent de la variante tirée (`variant.rs`) : aucune passation n'est la même.
//! La voiture réelle (pilotée, porteuse du `StreamFocus`) devient la **nouvelle** voiture ; l'ancienne est un double
//! visuel, qui prend sa place dès le début et reste fidèle à son DJ.

use super::timeline::RunStages;
use super::variant::HandoverVariant;
use crate::intro::ramp;

/// Plafond de durée d'une passation (s), toutes variantes confondues.
pub const HANDOVER_MAX_S: f32 = 52.0;

/// L'ancienne voiture, jouée par un double visuel sans pilote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleStages {
    /// Synchronisation : 1 pleine, 0 absente (le film de `car/sync.rs` à l'envers).
    pub presence: f32,
    /// Part du freinage vers la vitesse de croisière seule, 0 à 1.
    pub slow: f32,
    /// Part du déport vers son côté de la route, 0 à 1.
    pub lateral: f32,
    /// Part du décrochage final (elle ralentit et s'écarte en se désynchronisant), 0 à 1.
    pub fade: f32,
}

impl DoubleStages {
    /// Pas de double.
    pub const NONE: Self = Self {
        presence: 0.0,
        slow: 0.0,
        lateral: 0.0,
        fade: 0.0,
    };
}

/// La nouvelle voiture, la réelle : où elle se tient par rapport à l'ancienne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrivalStages {
    /// Distance derrière l'ancienne (m), de l'écart d'arrivée à 0.
    pub gap_behind_m: f32,
    /// La voiture réelle est menée par la frise (sinon elle est au pilote).
    pub scripted: bool,
}

impl ArrivalStages {
    /// Pas d'arrivée en cours.
    pub const NONE: Self = Self {
        gap_behind_m: 0.0,
        scripted: false,
    };
}

/// La passation en cours : son instant et sa variante, pour les systèmes qui la jouent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandoverFrame {
    /// Secondes depuis le début de la passation.
    pub t_s: f32,
    /// La variante tirée.
    pub variant: HandoverVariant,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Les étapes `t_s` secondes après le début du changement de DJ.
#[must_use]
pub fn handover_stages(t_s: f32, v: &HandoverVariant, reduce_motion: bool) -> (RunStages, DoubleStages, ArrivalStages) {
    let m = v.marks();
    let hidden = t_s >= m.folded;
    let glitch = |x: f32| if reduce_motion { 0.0 } else { x };
    // Le repli démarre lentement : les tours buguent d'abord sur place avant de s'enfoncer.
    let fold = ramp(t_s, 0.0, m.folded);
    let stages = RunStages {
        road: 1.0,
        // Phares et néons suivent la synchronisation de chaque voiture (`car/sync.rs`).
        lights: 1.0,
        build: if hidden { ramp(t_s, m.gantry + 5.0, m.end) } else { 1.0 },
        // Les fenêtres s'éteignent et s'allument avec leur tour (shader des façades).
        windows: 1.0,
        sky: 1.0,
        sign: if hidden {
            ramp(t_s, m.gantry + 0.5, m.gantry + 4.0)
        } else {
            1.0 - ramp(t_s, 0.0, 1.5)
        },
        drive: 1.0,
        collapse: if hidden { 0.0 } else { fold * fold * (2.0 - fold) },
        // La réelle est absente jusqu'à l'arrivée, puis se synchronise comme au démarrage.
        car_presence: ramp(t_s, m.arrival + 0.3, m.arrival + 5.8),
        music: 1.0,
        gantry: if t_s < m.gantry - 1.0 {
            0.0
        } else {
            ramp(t_s, m.gantry - 1.0, m.gantry + 6.0)
        },
        name_glitch: glitch(if t_s < m.gantry {
            0.0
        } else {
            1.0 - ramp(t_s, m.gantry + 0.5, m.gantry + 3.5)
        }),
        camera_waiting: false,
        camera_handover: t_s >= m.camera && t_s < m.gone + 1.0,
    };
    let double = DoubleStages {
        presence: 1.0 - ramp(t_s, m.fading, m.gone),
        slow: smooth(t_s / m.slowed),
        lateral: smooth((t_s - (m.arrival + 4.0)) / 3.5),
        fade: smooth((t_s - m.fading) / (m.gone - m.fading)),
    };
    let u = ((t_s - m.arrival) / super::variant::CATCH_S).clamp(0.0, 1.0);
    let arrival = ArrivalStages {
        gap_behind_m: v.gap_m * (1.0 - smooth(u)),
        scripted: t_s < m.release,
    };
    (stages, double, arrival)
}
