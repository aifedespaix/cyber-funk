//! La vie d'un panneau posé : distance, fondu, bug des portiques fantômes, radar (`step` et `glitch` de
//! `sign-manager.ts`).

use super::{Display, RoadFrame, SHOT_RANGE, SignState, Unit};
use crate::rng::hash;
use crate::signs::formats::Look;

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Une image de vie d'un panneau posé (`step`) : distance, fondu, bug, radar.
pub(super) fn step(unit: &mut Unit, frame: &RoadFrame<'_>) {
    let (Some(id), Some(mut state)) = (unit.format, unit.state) else {
        return;
    };
    let format = id.format();
    let ahead = if unit.leased {
        frame.car_z - unit.hang.z
    } else {
        frame.car_z - unit.z
    };
    state.range = ahead;
    let fade = smooth(format.range, format.range - 110.0, ahead) * smooth(-format.behind, -format.behind + 30.0, ahead);
    unit.opacity = fade as f32;
    unit.visible = fade > 0.01;
    if state.phantom {
        glitch(unit, &state, ahead, frame);
    }
    // Le radar prend sa photo quand la voiture passe sous le portique-caméra.
    state.radar = false;
    if format.look == Look::Camera {
        if !state.flashed && ahead < SHOT_RANGE && ahead > 0.0 {
            state.flashed = true;
            state.radar = true;
        }
        if ahead > SHOT_RANGE * 2.0 {
            state.flashed = false;
        }
    }
    unit.state = Some(state);
}

/// Un panneau que la voiture traverse : à l'approche son affichage saccade, se déchire et clignote de plus en plus
/// fort, il s'éteint pendant le passage, puis revient en bégayant. Seule la partie lumineuse bugue : les pieds, la
/// poutre et les câbles restent, comme de vrais objets. Rien ne se passe si la voiture n'est pas à la hauteur du panneau
/// (`glitch`).
fn glitch(unit: &mut Unit, _state: &SignState, ahead: f64, frame: &RoadFrame<'_>) {
    let centre = f64::from(unit.layout.y);
    let reach = f64::from(unit.height) / 2.0 + 3.0;
    let in_band = frame.altitude + 2.2 > centre - reach && frame.altitude - 0.7 < centre + reach;
    let level = if !in_band {
        0.0
    } else if ahead >= 0.0 {
        smooth(60.0, 8.0, ahead)
    } else {
        smooth(-34.0, -14.0, ahead)
    };
    if level <= 0.0 {
        if !unit.display.on || unit.glitching {
            unit.display = Display::NORMAL;
            unit.glitching = false;
        }
        return;
    }
    unit.glitching = true;
    if ahead < 6.0 && ahead > -14.0 {
        unit.display = Display {
            on: false,
            passing: true,
            ..Display::NORMAL
        };
        return;
    }
    let slot = unit.slot.map_or(0.0, |k| k as f64);
    let tick = (frame.time * 24.0).floor() + slot * 7.0;
    let off = hash(tick, 33.0) < level * 0.7;
    unit.display = Display {
        on: !off,
        dx: ((hash(tick, 34.0) - 0.5) * 7.0 * level) as f32,
        sx: (1.0 + (hash(tick, 35.0) - 0.5) * 0.5 * level) as f32,
        sy: (1.0 + (hash(tick, 36.0) - 0.35) * 0.35 * level) as f32,
        passing: false,
    };
}
