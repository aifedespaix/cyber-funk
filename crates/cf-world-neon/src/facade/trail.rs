//! La traîne de la voiture sur les façades : les seize derniers points où elle est passée, que le shader transforme en
//! une lueur qui s'attarde sur les fenêtres (objectif « suivi de la voiture » de la spec des façades vivantes).
//!
//! - **Absolu d'abord** : les points sont gardés en coordonnées absolues (`DVec3`), seule [`CarTrail::params`] les ramène
//!   dans le repère rendu : un recentrage de l'origine flottante ne déplace donc aucune lueur.
//! - **Aucune allocation** : un tampon circulaire de taille fixe, des tableaux sur la pile.
//! - **Sécurité photosensible** (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : la lueur ne peut pas produire de variation rapide
//!   de luminance. Chaque point s'éteint en [`TRAIL_LIFETIME_S`] (6 s) selon une courbe lisse, et le shader l'étale sur
//!   [`FALLOFF_M`] (40 m) : à 60 m/s une façade voit la lueur monter et retomber en plusieurs secondes, jamais en un
//!   front. Ce n'est pas un flash, donc pas de limiteur, mais ces deux bornes ne doivent pas être réduites.

use bevy::math::{DVec3, Vec3, Vec4};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, FloatingOrigin};

use super::FacadeLook;
use crate::car::{Car, CarState};

/// Nombre de points gardés (la taille du tableau d'uniformes du shader).
pub const TRAIL_POINTS: usize = 16;
/// Distance minimale entre deux points gardés, en m : une voiture arrêtée n'en ajoute aucun.
pub const MIN_STEP_M: f64 = 6.0;
/// Durée de vie d'un point, en s : au bout de ce temps sa lueur est nulle.
pub const TRAIL_LIFETIME_S: f64 = 6.0;
/// Demi-largeur de la lueur autour d'un point, en m (le `40` de `facade_trail.wesl`).
pub const FALLOFF_M: f32 = 40.0;
/// Force de la traîne (`car_tint.w`), 0 à 1. Constante tant que `FacadeLook` n'a pas son champ `car_follow`.
pub const CAR_FOLLOW: f32 = 1.0;

/// Les derniers points de passage de la voiture.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CarTrail {
    points: [DVec3; TRAIL_POINTS],
    times: [f64; TRAIL_POINTS],
    /// Emplacement du prochain point écrit.
    head: usize,
    len: usize,
    /// Couleur de la voiture (lumière linéaire) ; son `w` est la force de la traîne.
    tint: Vec4,
}

impl Default for CarTrail {
    fn default() -> Self {
        Self {
            points: [DVec3::ZERO; TRAIL_POINTS],
            times: [0.0; TRAIL_POINTS],
            head: 0,
            len: 0,
            tint: Vec4::ZERO,
        }
    }
}

impl CarTrail {
    /// Nombre de points gardés (au plus [`TRAIL_POINTS`]).
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Aucun point gardé.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Le dernier point écrit.
    fn last(&self) -> Option<DVec3> {
        (self.len > 0).then(|| self.points[(self.head + TRAIL_POINTS - 1) % TRAIL_POINTS])
    }

    /// Note la position absolue de la voiture à l'instant `now_s` ; n'ajoute un point que si elle a avancé d'au moins
    /// [`MIN_STEP_M`] depuis le dernier. Une position non finie est ignorée.
    pub fn record(&mut self, position: DVec3, now_s: f64) {
        if !position.is_finite() || !now_s.is_finite() {
            return;
        }
        if self.last().is_some_and(|last| last.distance(position) < MIN_STEP_M) {
            return;
        }
        self.points[self.head] = position;
        self.times[self.head] = now_s;
        self.head = (self.head + 1) % TRAIL_POINTS;
        self.len = (self.len + 1).min(TRAIL_POINTS);
    }

    /// Fixe la couleur de la voiture (lumière linéaire) et la force de la traîne.
    pub fn set_tint(&mut self, rgb: Vec3, strength: f32) {
        self.tint = Vec4::new(rgb.x, rgb.y, rgb.z, strength.clamp(0.0, 1.0));
    }

    /// `car_tint` du shader : `xyz` la couleur de la voiture, `w` la force de la traîne.
    #[must_use]
    pub fn tint(&self) -> Vec4 {
        self.tint
    }

    /// Les points pour le shader : `xyz` dans le repère rendu (absolu moins l'origine), `w` l'intensité restante, de 1
    /// (point tout frais) à 0 ([`TRAIL_LIFETIME_S`] plus tard) ; tout à zéro pour un emplacement vide ou éteint.
    #[must_use]
    pub fn params(&self, origin: DVec3, now_s: f64) -> [Vec4; TRAIL_POINTS] {
        let mut out = [Vec4::ZERO; TRAIL_POINTS];
        for (slot, (point, time)) in out.iter_mut().zip(self.points.iter().zip(&self.times)).take(self.len) {
            // Un temps qui recule (horloge imposée) compte pour un point frais : jamais d'intensité hors de 0..1.
            let age = (now_s - time).max(0.0);
            let left = (1.0 - age / TRAIL_LIFETIME_S).clamp(0.0, 1.0);
            // Courbe en carré : l'extinction est douce en fin de vie comme au début.
            let w = (left * left) as f32;
            if w > 0.0 {
                let rendered = (*point - origin).as_vec3();
                *slot = Vec4::new(rendered.x, rendered.y, rendered.z, w);
            }
        }
        out
    }
}

/// `CfSet::Present`, avant `update_material` : note la position de la voiture et sa couleur. Sans voiture (monde sans
/// `CarState`), la traîne n'est plus alimentée : ses points s'éteignent d'eux-mêmes.
pub(super) fn record_car_trail(
    time: Res<Time<Real>>,
    look: Res<FacadeLook>,
    cars: Query<(&AbsolutePosition, &CarState), With<Car>>,
    mut trail: ResMut<CarTrail>,
) {
    let Some((position, state)) = cars.iter().next() else {
        return;
    };
    let now_s = look.clock_s.unwrap_or_else(|| time.elapsed_secs_f64());
    trail.record(position.0, now_s);
    let glow = state.glow.current();
    trail.set_tint(Vec3::new(glow.red, glow.green, glow.blue), CAR_FOLLOW);
}

/// Les uniformes `trail` et `car_tint` de l'image, ou des zéros sans origine flottante.
pub(super) fn trail_uniforms(
    trail: &CarTrail,
    origin: Option<&FloatingOrigin>,
    now_s: f64,
) -> ([Vec4; TRAIL_POINTS], Vec4) {
    origin.map_or(([Vec4::ZERO; TRAIL_POINTS], Vec4::ZERO), |o| {
        (trail.params(o.origin(), now_s), trail.tint())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_is_part_of_the_facade_source() {
        use super::super::material::FACADE_SOURCE;
        for needle in [
            "fn trail_window_light(",
            "trail_window_light(in.world",
            "trail: array<vec4<f32>, 16>",
            "car_tint: vec4<f32>",
        ] {
            assert!(FACADE_SOURCE.contains(needle), "{needle} manque dans le shader");
        }
        // Les nouveaux champs viennent après `motion`, dans l'ordre de `FacadeParams`.
        let at = |s: &str| FACADE_SOURCE.find(s);
        assert!(at("motion: vec4<f32>") < at("trail: array<vec4<f32>, 16>"));
        assert!(at("trail: array<vec4<f32>, 16>") < at("car_tint: vec4<f32>"));
        assert!(FACADE_SOURCE.contains(&format!("TRAIL_FALLOFF_M: f32 = {FALLOFF_M:.1}")));
    }

    fn along_z(i: usize) -> DVec3 {
        DVec3::new(0.0, 0.0, -(i as f64) * 10.0)
    }

    #[test]
    fn record_skips_when_stopped() {
        let mut trail = CarTrail::default();
        let p = DVec3::new(5.0, 0.0, -100.0);
        for i in 0..200 {
            trail.record(p + DVec3::new(0.0, 0.0, 0.001 * f64::from(i)), f64::from(i) / 60.0);
        }
        assert_eq!(trail.len(), 1, "une voiture immobile laisse un seul point");
        assert!(trail.params(DVec3::ZERO, 3.0).iter().all(|v| v.is_finite()));
        trail.record(DVec3::new(f64::NAN, 0.0, 0.0), 4.0);
        trail.record(p, f64::NAN);
        assert_eq!(trail.len(), 1, "une position non finie est ignorée");
    }

    #[test]
    fn ring_wraps_and_keeps_latest_16() {
        let mut trail = CarTrail::default();
        for i in 0..40 {
            trail.record(along_z(i), 0.0);
        }
        assert_eq!(trail.len(), TRAIL_POINTS);
        let out = trail.params(DVec3::ZERO, 0.0);
        let mut zs: Vec<f32> = out.iter().map(|v| v.z).collect();
        zs.sort_by(f32::total_cmp);
        let expected: Vec<f32> = (24..40).rev().map(|i| -(i as f32) * 10.0).collect();
        assert_eq!(zs, expected, "les 16 derniers points, rien d'autre");
    }

    #[test]
    fn intensity_decays_with_age() {
        let mut trail = CarTrail::default();
        trail.record(DVec3::ZERO, 10.0);
        let w = |now| trail.params(DVec3::ZERO, now)[0].w;
        assert_eq!(w(10.0), 1.0);
        let mut last = w(10.0);
        for k in 1..=60 {
            let now = 10.0 + f64::from(k) * 0.1;
            let v = w(now);
            assert!(v <= last, "l'intensité ne remonte jamais ({last} -> {v} à {now})");
            // Pas de saut : au plus 20 % d'intensité par dixième de seconde.
            assert!(last - v < 0.2, "saut de {} en 0,1 s", last - v);
            last = v;
        }
        assert_eq!(w(16.0), 0.0);
        assert_eq!(w(1.0e9), 0.0, "un énorme dt éteint tout sans NaN");
        assert_eq!(w(5.0), 1.0, "horloge à l'envers : point frais, jamais plus de 1");
        assert_eq!(
            trail.params(DVec3::ZERO, 16.0)[0],
            Vec4::ZERO,
            "un point éteint est nul"
        );
        assert_eq!(trail.params(DVec3::ZERO, 10.0)[1], Vec4::ZERO, "emplacement vide");
    }

    #[test]
    fn params_follow_origin_rebase() {
        let mut trail = CarTrail::default();
        let p = DVec3::new(120.0, 2.0, -400_000.0);
        trail.record(p, 0.0);
        let before = trail.params(DVec3::new(0.0, 0.0, -399_000.0), 0.0)[0];
        let after = trail.params(DVec3::new(0.0, 0.0, -400_000.0), 0.0)[0];
        assert_eq!(
            before.z + 1000.0,
            after.z,
            "le recentrage décale le repère rendu de 1 000 m"
        );
        assert_eq!(before.x, after.x);
        // La position absolue, elle, n'a pas bougé.
        assert_eq!(trail.last(), Some(p));
    }
}
