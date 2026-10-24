//! Plafond de cadence des coupes (sécurité du public, règle 7 : WCAG 2.3.1).
//!
//! Une coupe franche change toute l'image d'un coup. Sillage ne limitait pas leur cadence : le plan `strobe` en enchaîne
//! une à chaque temps (~2/s à 128 BPM, ~3/s à 174 BPM, davantage si le tempo détecté s'emballe). Le réalisateur plafonne
//! donc **toutes** les coupes, quel que soit le plan qui les demande, à [`MAX_CUTS_PER_SECOND`] par seconde glissante.
//! Deux sortes de coupes comptent : la transition `Cut` du réalisateur, et le saut de pose qu'un plan s'inflige à
//! lui-même (le changement d'angle de `strobe`). Une coupe refusée n'est pas perdue, elle est retardée : une transition
//! `Cut` devient un fondu, et un plan qui saute garde sa pose précédente jusqu'à ce que la fenêtre se libère.

use bevy::math::DVec3;
use cf_render::CameraPose;

/// Plafond de cadence des coupes, en coupes par seconde glissante (même plafond que le limiteur de flashs).
pub const MAX_CUTS_PER_SECOND: u32 = 3;
/// Largeur de la fenêtre glissante (s).
pub const WINDOW_SECONDS: f64 = 1.0;
/// Saut de position (m, dans le repère de la voiture) au-delà duquel une pose est une coupe. Un plan qui suit la voiture
/// ne bouge pas de cela d'une image à l'autre (la voiture qui file à 70 m/s est déjà retranchée) ; les angles de `strobe`
/// sont à 3,8 m au moins les uns des autres.
const JUMP_METERS: f64 = 2.0;
/// Part du saut tolérée par seconde de temps de l'image (m/s) : un accroc d'image ne fabrique pas de fausse coupe.
const JUMP_METERS_PER_SECOND: f64 = 100.0;
/// Saut d'orientation (rad) : un plan qui pivote vite ne tourne pas de 20° en une image.
const JUMP_ANGLE: f32 = 0.35;
/// Saut de champ de vision (°).
const JUMP_FOV: f32 = 15.0;

const SLOTS: usize = MAX_CUTS_PER_SECOND as usize;

/// Les dernières coupes admises, sur une seconde glissante. Taille fixe : aucune allocation.
#[derive(Debug, Clone, Copy)]
pub struct CutLimiter {
    at: [f64; SLOTS],
    next: usize,
}

impl Default for CutLimiter {
    fn default() -> Self {
        Self {
            at: [f64::NEG_INFINITY; SLOTS],
            next: 0,
        }
    }
}

impl CutLimiter {
    /// Oublie les coupes passées (nouveau départ du réalisateur).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Une coupe à l'instant `now` serait-elle admise ? Ne l'enregistre pas.
    #[must_use]
    pub fn allows(&self, now: f64) -> bool {
        // La plus ancienne des `SLOTS` dernières coupes doit être sortie de la fenêtre : `now − t ≥ 1 s`.
        let oldest = self.at[self.next];
        now - oldest >= WINDOW_SECONDS
    }

    /// Demande une coupe à l'instant `now` : l'enregistre et renvoie vrai si la fenêtre le permet.
    pub fn try_cut(&mut self, now: f64) -> bool {
        if !self.allows(now) {
            return false;
        }
        self.at[self.next] = now;
        self.next = (self.next + 1) % SLOTS;
        true
    }
}

/// La pose de cette image est-elle une coupe par rapport à la précédente ? Les deux poses sont comparées dans le
/// repère de la voiture (`car`, absolue) : suivre la voiture à 70 m/s n'est pas un saut.
#[must_use]
pub fn is_jump(before: &CameraPose, before_car: DVec3, after: &CameraPose, car: DVec3, dt: f32) -> bool {
    let moved = ((after.position - car) - (before.position - before_car)).length();
    let limit = JUMP_METERS + JUMP_METERS_PER_SECOND * f64::from(dt.max(0.0));
    moved > limit
        || before.rotation.angle_between(after.rotation) > JUMP_ANGLE
        || (after.fov_deg - before.fov_deg).abs() > JUMP_FOV
        || before.cabin != after.cabin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trois_coupes_par_seconde_glissante() {
        let mut limiter = CutLimiter::default();
        assert!(limiter.try_cut(0.0));
        assert!(limiter.try_cut(0.3));
        assert!(limiter.try_cut(0.6));
        assert!(!limiter.try_cut(0.9), "une quatrième dans la même seconde");
        assert!(!limiter.try_cut(0.999));
        // La première sort de la fenêtre à 1,0 s pile ; elle libère une place, pas trois.
        assert!(limiter.try_cut(1.0));
        assert!(!limiter.try_cut(1.1));
        assert!(limiter.try_cut(1.3));
    }

    #[test]
    fn un_refus_ne_consomme_rien() {
        let mut limiter = CutLimiter::default();
        for t in [0.0, 0.1, 0.2] {
            assert!(limiter.try_cut(t));
        }
        for t in [0.3, 0.5, 0.9] {
            assert!(!limiter.try_cut(t));
        }
        assert!(limiter.try_cut(1.0));
    }

    #[test]
    fn suivre_la_voiture_n_est_pas_un_saut() {
        let before = CameraPose {
            position: DVec3::new(0.0, 3.0, 8.0),
            ..CameraPose::default()
        };
        let mut after = before;
        after.position.z -= 70.0 / 60.0;
        let car_step = DVec3::new(0.0, 0.0, -70.0 / 60.0);
        assert!(!is_jump(&before, DVec3::ZERO, &after, car_step, 1.0 / 60.0));
        // Même position absolue, voiture partie loin : la caméra a bien sauté dans le repère de la voiture.
        after.position = before.position;
        assert!(is_jump(
            &before,
            DVec3::ZERO,
            &after,
            DVec3::new(0.0, 0.0, -6.0),
            1.0 / 60.0
        ));
    }
}
