//! Passage entre deux plans (`cinema/transition.ts`) : coupe, fondu, envolée, panoramique éclair.

use cf_render::CameraPose;

use crate::kit::FOV_MAX;
use crate::pose::{mix_pose, smootherstep};
use crate::types::TransitionKind;
use crate::world::CinemaWorld;

/// Envolée : l'arc monte de ce rapport de la distance parcourue, borné.
const SWOOP_HEIGHT: f64 = 0.4;
const SWOOP_MIN: f64 = 8.0;
const SWOOP_MAX: f64 = 80.0;
const SWOOP_FOV: f32 = 10.0;
const WHIP_SECONDS: f32 = 0.3;
const WHIP_FOV: f32 = 15.0;
/// Fondu : un petit arc, pour contourner la voiture et les toits plutôt que de les traverser en ligne droite.
const BLEND_ARC: f64 = 0.22;
const BLEND_ARC_MAX: f64 = 14.0;
/// Au-delà de cette distance entre deux poses (m), un fondu devient une envolée.
pub const SWOOP_DISTANCE: f64 = 60.0;

/// Durée d'une transition (s), liée au tempo : un nombre de temps, borné. `calm` : plus lent.
#[must_use]
pub fn transition_seconds(kind: TransitionKind, world: &CinemaWorld, calm: bool) -> f32 {
    let beat = match world.beat.bpm {
        Some(bpm) if bpm > 0.0 => 60.0 / bpm,
        _ => 0.5,
    };
    match kind {
        TransitionKind::Cut => 0.0,
        TransitionKind::Whip => WHIP_SECONDS,
        TransitionKind::Swoop => (beat * 6.0).clamp(2.2, 4.2) * if calm { 1.3 } else { 1.0 },
        TransitionKind::Blend => {
            // Un vrai mouvement de caméra : 8 temps au repos, 6 en pleine énergie (3 à 5,5 s).
            let beats = if world.intensity > 0.6 { 6.0 } else { 8.0 };
            let seconds = (beat * beats).clamp(3.0, 5.5);
            if calm { (seconds * 1.4).max(4.5) } else { seconds }
        }
    }
}

/// Passage entre DEUX plans vivants : le réalisateur met à jour les deux à chaque image pendant la transition, ici on
/// ne fait que mélanger leurs poses.
#[derive(Debug, Clone)]
pub struct Transition {
    /// Une transition est en cours.
    pub active: bool,
    /// Genre de la transition en cours (ou de la dernière).
    pub kind: TransitionKind,
    duration: f32,
    elapsed: f32,
    height: f64,
    arc: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            active: false,
            kind: TransitionKind::Cut,
            duration: 0.0,
            elapsed: 0.0,
            height: 0.0,
            arc: 0.0,
        }
    }
}

impl Transition {
    /// Commence une transition de `seconds` entre deux poses distantes de `distance` m.
    pub fn begin(&mut self, kind: TransitionKind, seconds: f32, distance: f64) {
        self.kind = kind;
        self.duration = seconds;
        self.elapsed = 0.0;
        self.height = (distance * SWOOP_HEIGHT).clamp(SWOOP_MIN, SWOOP_MAX);
        self.arc = (distance * BLEND_ARC).clamp(0.0, BLEND_ARC_MAX);
        self.active = kind != TransitionKind::Cut && seconds > 0.0;
    }

    /// Avance d'une image et écrit le mélange dans `out`. Renvoie vrai quand la transition est finie.
    pub fn step(&mut self, dt: f32, from: &CameraPose, to: &CameraPose, out: &mut CameraPose) -> bool {
        self.elapsed += dt;
        let t = if self.duration > 0.0 {
            (self.elapsed / self.duration).min(1.0)
        } else {
            1.0
        };
        mix_pose(out, from, to, smootherstep(t));
        let bump = (std::f32::consts::PI * t).sin();
        match self.kind {
            TransitionKind::Swoop => {
                out.position.y += f64::from(bump) * self.height;
                out.fov_deg += bump * SWOOP_FOV;
            }
            TransitionKind::Blend => out.position.y += f64::from(bump) * self.arc,
            TransitionKind::Whip => out.fov_deg += bump * WHIP_FOV,
            TransitionKind::Cut => {}
        }
        out.fov_deg = out.fov_deg.min(FOV_MAX);
        // La vue au volant ne s'affiche qu'une fois arrivés : sinon l'habitacle flotterait à l'extérieur.
        let done = t >= 1.0;
        out.cabin = done && to.cabin;
        if done {
            self.active = false;
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::DVec3;

    use super::*;

    #[test]
    fn les_durees_suivent_le_tempo() {
        let mut world = CinemaWorld::default();
        world.beat.bpm = Some(120.0);
        world.intensity = 0.4;
        assert!((transition_seconds(TransitionKind::Blend, &world, false) - 4.0).abs() < 1e-6);
        world.intensity = 0.8;
        assert!((transition_seconds(TransitionKind::Blend, &world, false) - 3.0).abs() < 1e-6);
        assert!((transition_seconds(TransitionKind::Swoop, &world, false) - 3.0).abs() < 1e-6);
        assert!(transition_seconds(TransitionKind::Cut, &world, false).abs() < 1e-9);
        assert!((transition_seconds(TransitionKind::Whip, &world, true) - 0.3).abs() < 1e-6);
        assert!((transition_seconds(TransitionKind::Blend, &world, true) - 4.5).abs() < 1e-6);
    }

    #[test]
    fn l_envolee_monte_en_arc_et_finit_sur_l_arrivee() {
        let from = CameraPose::default();
        let to = CameraPose {
            position: DVec3::new(100.0, 0.0, 0.0),
            ..CameraPose::default()
        };
        let mut out = CameraPose::default();
        let mut tr = Transition::default();
        tr.begin(TransitionKind::Swoop, 2.0, 100.0);
        assert!(tr.active);
        assert!(!tr.step(1.0, &from, &to, &mut out));
        assert!((out.position.y - 40.0).abs() < 1e-6, "{}", out.position.y);
        assert!(tr.step(1.0, &from, &to, &mut out));
        // sin(π) n'est pas nul en f32 : quelques micromètres d'arc restent.
        assert!((out.position - to.position).length() < 1e-4);
        assert!(!tr.active);
    }

    #[test]
    fn la_coupe_est_instantanee() {
        let mut tr = Transition::default();
        tr.begin(TransitionKind::Cut, 0.0, 10.0);
        assert!(!tr.active);
        let to = CameraPose {
            cabin: true,
            ..CameraPose::default()
        };
        let mut out = CameraPose::default();
        assert!(tr.step(0.016, &CameraPose::default(), &to, &mut out));
        assert!(out.cabin);
    }
}
