//! Les petites fonctions des cascades (`stunts/math.ts`) : marches douces, bosses, rotations, dérivée lissée.
//! Les scripts de cascades (J7-07) les partagent.

use bevy::math::DVec3;

use super::world::StuntWorld;
use crate::pilot::rand::Rand;
use crate::traffic::CraftId;

/// Un tour complet, rad.
pub const TAU: f64 = std::f64::consts::TAU;

/// Interpolation linéaire.
#[must_use]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Pile ou face signé : -1 ou +1 (`sign()`).
pub fn sign(rng: &mut Rand) -> f64 {
    if rng.unit() < 0.5 { -1.0 } else { 1.0 }
}

/// Marche douce de 0 (avant `a`) à 1 (après `b`), en temps musicaux ou en mètres.
#[must_use]
pub fn between(u: f64, a: f64, b: f64) -> f64 {
    let t = ((u - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bosse : monte de `a` à `b`, tient jusqu'à `c`, redescend jusqu'à `d`.
#[must_use]
#[allow(clippy::many_single_char_names)] // a, b, c, d : les quatre bornes de la bosse, comme dans `math.ts`.
pub fn bump(u: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    between(u, a, b) * (1.0 - between(u, c, d))
}

/// Rotation douce de 0 à 1 (plus posée aux extrémités que [`between`]).
#[must_use]
pub fn turn(u: f64, a: f64, b: f64) -> f64 {
    let t = ((u - a) / (b - a)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Déplace un véhicule scénarisé et lui donne la vitesse de ce déplacement (cap, traînée). Sans effet si `craft` est
/// `None` ou n'est plus vivant (la flotte a pu le recycler).
pub fn move_craft(world: &mut dyn StuntWorld, craft: Option<CraftId>, x: f64, y: f64, z: f64, dt: f64) {
    let Some(craft) = craft.and_then(|id| world.craft_mut(id)) else {
        return;
    };
    let p = craft.position;
    if dt > 0.0 {
        craft.velocity = DVec3::new((x - p.x) / dt, (y - p.y) / dt, (z - p.z) / dt).as_vec3();
    }
    craft.position = DVec3::new(x, y, z);
}

/// Dérivée lissée d'une grandeur : le nez suit la montée, la caisse se couche dans le virage.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rate {
    last: Option<f64>,
    value: f64,
}

impl Rate {
    /// Vitesse de variation (unités par seconde) de `x`.
    pub fn follow(&mut self, x: f64, dt: f64) -> f64 {
        let Some(last) = self.last.filter(|_| dt > 0.0) else {
            self.last = Some(x);
            return 0.0;
        };
        let rate = (x - last) / dt;
        self.last = Some(x);
        self.value += (rate - self.value) * (1.0 - (-dt * 7.0).exp());
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marches_et_bosses_bornees() {
        assert!(between(-1.0, 0.0, 1.0).abs() < 1e-12);
        assert!((between(2.0, 0.0, 1.0) - 1.0).abs() < 1e-12);
        assert!((between(0.5, 0.0, 1.0) - 0.5).abs() < 1e-12);
        assert!((bump(2.0, 0.0, 1.0, 3.0, 4.0) - 1.0).abs() < 1e-12);
        assert!(bump(5.0, 0.0, 1.0, 3.0, 4.0).abs() < 1e-12);
        assert!(turn(0.0, 0.0, 1.0).abs() < 1e-12 && (turn(1.0, 0.0, 1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn la_derivee_suit_une_rampe() {
        let mut rate = Rate::default();
        assert!(rate.follow(0.0, 0.016).abs() < 1e-12);
        let mut last = 0.0;
        for i in 1..300 {
            last = rate.follow(f64::from(i) * 0.016 * 2.0, 0.016);
        }
        assert!((last - 2.0).abs() < 0.05, "{last}");
    }
}
