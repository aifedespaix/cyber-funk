//! La frise du changement de DJ : le quartier se replie, la voiture réelle (la nouvelle) revient de loin derrière et
//! double un double visuel de l'ancienne, qui s'éteint ; le portique annonce le nouveau DJ ; le quartier se reconstruit.
//! Pur : aucune dépendance à Bevy.
//!
//! ```text
//!  0 s            1 s            ~7 s        10 s        13-18 s         18-27 s
//!  ├─ repli ───────────────────────────────────┤  caché     ├─ portique ─┤├─ quartier ─┤
//!  tir de la passation du chef du spectacle (fondu de palette fini avant 18 s)
//!        échange : le double prend la place de l'ancienne ; la réelle repart derrière à gauche
//!        ├──── la nouvelle rattrape ───────┤ dépasse (10 s) ; l'ancienne ralentit, glitche, s'éteint (11-13 s)
//! ```

use super::timeline::{GANTRY_END_S, GANTRY_START_S, RunStages, mix};
use crate::intro::ramp;

/// Durée du changement de DJ (s).
pub const HANDOVER_S: f32 = 27.0;
/// Instant où le chef du spectacle reçoit la passation (s) : son fondu de palette (6 à 10 s, après une attente de temps
/// fort d'au plus 4 s) est fini quand le quartier est caché et que le portique arrive.
pub const HANDOVER_FIRE_S: f32 = 0.0;
/// Instant où le double prend la place de l'ancienne voiture et où la réelle repart derrière (s).
pub const HANDOVER_SWAP_S: f32 = 1.0;
/// Instant où la nouvelle voiture passe devant l'ancienne (s).
pub const OVERTAKE_S: f32 = 10.0;
/// Fin du repli : tout est caché et rendu à l'état d'attente (s).
const FOLD_END_S: f32 = 10.5;
/// Distance derrière l'ancienne d'où la nouvelle voiture revient (m).
pub const ARRIVAL_GAP_M: f32 = 60.0;
/// Écart latéral à gauche pendant le rattrapage (m).
pub const ARRIVAL_LATERAL_M: f32 = 3.5;
/// Avance de la nouvelle voiture à la fin du dépassement (m).
const OVERTAKE_LEAD_M: f32 = 15.0;

/// L'ancienne voiture, jouée par un double visuel sans pilote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleStages {
    /// Présence : 0 absente, 1 pleine (l'extinction « télé » passe 1 → 0).
    pub presence: f32,
    /// Part du déport vers le côté droit de la route, 0 à 1.
    pub pull_right: f32,
    /// Part du freinage, 0 à 1.
    pub brake: f32,
    /// Bug / clignotement avant l'extinction, 0 à 1 (passe par le limiteur).
    pub glitch: f32,
}

impl DoubleStages {
    /// Pas de double.
    pub const NONE: Self = Self {
        presence: 0.0,
        pull_right: 0.0,
        brake: 0.0,
        glitch: 0.0,
    };
}

/// La nouvelle voiture, la réelle : où elle se tient par rapport à l'ancienne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrivalStages {
    /// Distance derrière l'ancienne (m) ; négative une fois qu'elle l'a doublée.
    pub gap_behind_m: f32,
    /// Écart à gauche de l'ancienne (m).
    pub lateral_m: f32,
    /// Part de la surcharge de vitesse pour rattraper, 0 à 1.
    pub boost: f32,
}

impl ArrivalStages {
    /// Pas d'arrivée en cours.
    pub const NONE: Self = Self {
        gap_behind_m: 0.0,
        lateral_m: 0.0,
        boost: 0.0,
    };
}

/// Les étapes `t_s` secondes après le début du changement de DJ.
#[must_use]
pub fn handover_stages(t_s: f32, reduce_motion: bool) -> (RunStages, DoubleStages, ArrivalStages) {
    let hidden = t_s >= FOLD_END_S;
    let swapped = t_s >= HANDOVER_SWAP_S;
    let glitch = |v: f32| if reduce_motion { 0.0 } else { v };
    // Le portique passe au même rythme qu'à l'intro, décalé de 1 s : il apparaît à 13 s et se replie à 19 s.
    let g0 = GANTRY_START_S + 1.0;
    let g1 = GANTRY_END_S + 1.0;
    let stages = RunStages {
        road: 1.0,
        lights: 1.0,
        build: if hidden { ramp(t_s, g1, HANDOVER_S) } else { 1.0 },
        windows: 1.0,
        sky: 1.0,
        sign: if hidden {
            ramp(t_s, g0 + 0.5, g0 + 4.0)
        } else {
            1.0 - ramp(t_s, 0.0, 1.5)
        },
        drive: 1.0,
        collapse: if hidden { 0.0 } else { ramp(t_s, 0.0, OVERTAKE_S) },
        // La réelle disparaît à l'échange (le double prend sa place), puis se forme loin derrière.
        car_presence: if swapped {
            ramp(t_s, HANDOVER_SWAP_S + 0.2, HANDOVER_SWAP_S + 1.2)
        } else {
            1.0
        },
        music: 1.0,
        gantry: if t_s < g0 - 1.0 {
            0.0
        } else {
            ramp(t_s, g0 - 1.0, g1 + 1.0)
        },
        name_glitch: glitch(if t_s < g0 {
            0.0
        } else {
            1.0 - ramp(t_s, g0 + 0.5, g0 + 3.5)
        }),
        camera_waiting: false,
        camera_handover: swapped && t_s < OVERTAKE_S,
    };
    let double = DoubleStages {
        presence: if swapped { 1.0 - ramp(t_s, 11.0, 13.0) } else { 0.0 },
        pull_right: ramp(t_s, HANDOVER_SWAP_S, 6.0),
        brake: ramp(t_s, 6.0, 13.0),
        glitch: glitch(ramp(t_s, 8.0, 10.0) * (1.0 - ramp(t_s, 11.0, 13.0))),
    };
    let arrival = if swapped {
        ArrivalStages {
            gap_behind_m: mix(ARRIVAL_GAP_M, 0.0, ramp(t_s, HANDOVER_SWAP_S + 0.2, 7.0))
                - OVERTAKE_LEAD_M * ramp(t_s, 7.0, OVERTAKE_S),
            lateral_m: ARRIVAL_LATERAL_M * (1.0 - ramp(t_s, 8.0, 12.0)),
            boost: ramp(t_s, HANDOVER_SWAP_S + 0.2, 6.0) * (1.0 - ramp(t_s, 7.0, OVERTAKE_S)),
        }
    } else {
        ArrivalStages {
            gap_behind_m: ARRIVAL_GAP_M,
            ..ArrivalStages::NONE
        }
    };
    (stages, if swapped { double } else { DoubleStages::NONE }, arrival)
}
