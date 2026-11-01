//! La traîne de la voiture sur les façades : les seize derniers points où elle est passée, que le shader transforme en
//! une lueur qui s'attarde sur les fenêtres (objectif « suivi de la voiture » de la spec des façades vivantes).
//!
//! - **Absolu d'abord** : les points sont gardés en coordonnées absolues (`DVec3`), seule [`CarTrail::params`] les ramène
//!   dans le repère rendu : un recentrage de l'origine flottante ne déplace donc aucune lueur.
//! - **Aucune allocation** : un tampon circulaire de taille fixe, des tableaux sur la pile.
//! - **Sécurité photosensible** (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : la lueur ne peut pas produire de variation rapide
//!   de luminance. Un point naît en fondu ([`FADE_IN_S`], 0,4 s : jamais de saut quand la voiture repart), s'éteint au
//!   plus tard en [`TRAIL_LIFETIME_S`] (6 s) selon une courbe lisse, et le shader l'étale sur [`FALLOFF_M`] (40 m).
//!   Le tampon ne tient que 16 points espacés de 6 m au moins : à 60 m/s (un point tous les 0,1 s) il ne couvre que
//!   1,6 s, soit environ 100 m, et le plus ancien est réécrit à environ 54 % d'intensité. Ce n'est pas un saut visible :
//!   son voisin, à 6 m, éclaire presque autant (0,98 de recouvrement à 40 m d'étalement), et un test borne le pas d'une
//!   image à l'autre. La durée de 6 s n'est donc atteinte qu'à basse vitesse (moins de 16 points en 6 s, soit moins
//!   de 16 m/s). Ce n'est pas un flash, donc pas de limiteur, mais ces bornes ne doivent pas être réduites.
//! - **Continuité** : un saut d'horloge (bascule de `FacadeLook::clock_s`) décale les dates des points du même écart, les
//!   âges ne changent pas ; une téléportation (plus de [`TELEPORT_M`] entre deux positions) vide le tampon.

use bevy::math::{DVec3, Vec3, Vec4};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, FloatingOrigin};

use super::FacadeLook;
use super::systems::unit;
use crate::car::{Car, CarState};

/// Nombre de points gardés (la taille du tableau d'uniformes du shader).
pub const TRAIL_POINTS: usize = 16;
/// Distance minimale entre deux points gardés, en m : une voiture arrêtée n'en ajoute aucun.
pub const MIN_STEP_M: f64 = 6.0;
/// Durée de vie d'un point, en s : au bout de ce temps sa lueur est nulle.
pub const TRAIL_LIFETIME_S: f64 = 6.0;
/// Durée du fondu d'entrée d'un point, en s : la lueur d'un point neuf monte lisse de 0 à 1 (la voiture qui repart après un
/// arrêt ne fait jamais sauter la façade voisine).
pub const FADE_IN_S: f64 = 0.4;
/// Écart de position, en m, au-delà duquel la voiture a été téléportée : les anciens points n'ont plus de lien avec elle.
pub const TELEPORT_M: f64 = 500.0;
/// Écart d'horloge vers l'avant, en s, au-delà duquel on y voit une bascule de source (et non une image lente).
pub const CLOCK_JUMP_S: f64 = 30.0;
/// Demi-largeur de la lueur autour d'un point, en m (le `40` de `facade_trail.wesl`).
pub const FALLOFF_M: f32 = 40.0;

/// Les derniers points de passage de la voiture.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CarTrail {
    points: [DVec3; TRAIL_POINTS],
    times: [f64; TRAIL_POINTS],
    /// Emplacement du prochain point écrit.
    head: usize,
    len: usize,
    /// Horloge de la dernière écriture, pour repérer une bascule de source.
    last_now: f64,
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
            last_now: 0.0,
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
        if self.len > 0 {
            // Horloge qui recule ou qui saute : on décale les dates, les âges restent ceux de l'image d'avant.
            let jump = now_s - self.last_now;
            if jump < 0.0 || jump > CLOCK_JUMP_S {
                for t in &mut self.times {
                    *t += jump;
                }
            }
        }
        self.last_now = now_s;
        if let Some(last) = self.last() {
            if last.distance(position) > TELEPORT_M {
                // Téléportation ou réapparition : l'ancienne traîne ne mène nulle part.
                self.head = 0;
                self.len = 0;
            } else if last.distance(position) < MIN_STEP_M {
                return;
            }
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
    /// (point au bout de [`FADE_IN_S`], monté en fondu depuis 0) à 0 ([`TRAIL_LIFETIME_S`] après sa naissance) ; tout à zéro pour un emplacement vide ou éteint.
    #[must_use]
    pub fn params(&self, origin: DVec3, now_s: f64) -> [Vec4; TRAIL_POINTS] {
        let mut out = [Vec4::ZERO; TRAIL_POINTS];
        for (slot, (point, time)) in out.iter_mut().zip(self.points.iter().zip(&self.times)).take(self.len) {
            // Un temps qui recule (horloge imposée) compte pour un point naissant : jamais d'intensité hors de 0..1.
            let age = (now_s - time).max(0.0);
            let left = (1.0 - age / TRAIL_LIFETIME_S).clamp(0.0, 1.0);
            // Fondu d'entrée (smoothstep) puis courbe en carré : ni montée ni extinction brusque.
            let rise = (age / FADE_IN_S).clamp(0.0, 1.0);
            let w = (left * left * rise * rise * (3.0 - 2.0 * rise)) as f32;
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
    trail.set_tint(Vec3::new(glow.red, glow.green, glow.blue), unit(look.car_follow));
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
        let out = trail.params(DVec3::ZERO, 1.0);
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
        assert_eq!(w(10.0), 0.0, "un point neuf naît à 0 (fondu d'entrée)");
        let peak = w(10.0 + FADE_IN_S);
        assert!(peak > 0.8 && peak <= 1.0, "pic après le fondu : {peak}");
        let mut last = peak;
        for k in 5..=60 {
            let now = 10.0 + f64::from(k) * 0.1;
            let v = w(now);
            assert!(v <= last, "l'intensité ne remonte jamais ({last} -> {v} à {now})");
            // Pas de saut : au plus 20 % d'intensité par dixième de seconde.
            assert!(last - v < 0.2, "saut de {} en 0,1 s", last - v);
            last = v;
        }
        assert_eq!(w(16.0), 0.0);
        assert_eq!(w(1.0e9), 0.0, "un énorme dt éteint tout sans NaN");
        assert_eq!(w(5.0), 0.0, "horloge à l'envers : point naissant, jamais plus de 1");
        assert_eq!(
            trail.params(DVec3::ZERO, 16.0)[0],
            Vec4::ZERO,
            "un point éteint est nul"
        );
        assert_eq!(trail.params(DVec3::ZERO, 10.0)[1], Vec4::ZERO, "emplacement vide");
    }

    #[test]
    fn new_point_fades_in_without_a_step() {
        // Voiture arrêtée dix secondes (sa traîne est éteinte), puis elle repart : aucune façade ne saute.
        let mut trail = CarTrail::default();
        trail.record(DVec3::ZERO, 0.0);
        let mut last = 0.0_f32;
        let mut peak = 0.0_f32;
        for i in 0..120 {
            let now = 10.0 + f64::from(i) / 60.0;
            trail.record(DVec3::new(0.0, 0.0, -7.0), now);
            let w = trail.params(DVec3::ZERO, now).iter().map(|v| v.w).fold(0.0, f32::max);
            assert!((w - last).abs() <= 0.07, "saut de {} à t = {now}", (w - last).abs());
            peak = peak.max(w);
            last = w;
        }
        assert!(peak > 0.8, "le point a fini par briller : {peak}");
    }

    /// Miroir de `trail_glow` (`facade_trail.wesl`) : le maximum, sur les points, de l'intensité par le fondu gaussien de la
    /// distance (l'altitude compte pour moitié).
    fn mirror_glow(points: &[Vec4; TRAIL_POINTS], at: Vec3) -> f32 {
        points
            .iter()
            .map(|t| {
                let d = (at - Vec3::new(t.x, t.y, t.z)) * Vec3::new(1.0, 0.5, 1.0);
                let x = d.length() / FALLOFF_M;
                t.w * (-x * x).exp()
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn glow_at_a_facade_is_smooth_while_the_car_passes() {
        // La voiture reste dix secondes arrêtée, puis passe à 60 m/s devant une façade à 12 m de la route.
        let facade = Vec3::new(12.0, 8.0, -400.0);
        let dt = 1.0 / 60.0;
        let mut trail = CarTrail::default();
        let mut last = 0.0_f32;
        let mut peak = 0.0_f32;
        let mut worst = 0.0_f32;
        for i in 0..1800 {
            let now = f64::from(i) * dt;
            let z = if now < 10.0 { 0.0 } else { -(now - 10.0) * 60.0 };
            trail.record(DVec3::new(0.0, 0.0, z), now);
            let g = mirror_glow(&trail.params(DVec3::ZERO, now), facade);
            worst = worst.max((g - last).abs());
            peak = peak.max(g);
            last = g;
        }
        assert!(peak > 0.5, "la façade s'éclaire au passage : {peak}");
        // Fondu d'entrée de 0,4 s (pente maximale 3,75 par s, soit 0,0625 par image) ; l'écrasement du plus ancien point
        // (0,54 d'intensité, voisin à 6 m) coûte moins de 0,03.
        assert!(worst <= 0.07, "pas maximal d'une image à l'autre : {worst}");
    }

    #[test]
    fn clock_source_switch_does_not_jump() {
        let mut trail = CarTrail::default();
        for i in 0..6 {
            trail.record(along_z(i), 1000.0 + i as f64 * 0.1);
        }
        let before = trail.params(DVec3::ZERO, 1001.0);
        // Horloge imposée (retour à 5 s), puis retour à l'horloge réelle (+ 5 000 s) : voiture arrêtée à chaque fois.
        let stop = along_z(5);
        trail.record(stop, 5.5);
        assert_eq!(
            trail.params(DVec3::ZERO, 5.5 + 0.5),
            before,
            "bascule vers l'horloge imposée"
        );
        trail.record(stop, 5000.0);
        assert_eq!(
            trail.params(DVec3::ZERO, 5000.0 + 0.5),
            before,
            "bascule vers l'horloge réelle"
        );
    }

    #[test]
    fn teleport_clears_the_trail() {
        let mut trail = CarTrail::default();
        for i in 0..10 {
            trail.record(along_z(i), i as f64 * 0.1);
        }
        assert_eq!(trail.len(), 10);
        let far = DVec3::new(0.0, 0.0, -90_000.0);
        trail.record(far, 1.0);
        assert_eq!(trail.len(), 1, "seul le point d'arrivée reste");
        let lit = trail.params(DVec3::ZERO, 2.0).iter().filter(|v| v.w > 0.0).count();
        assert_eq!(lit, 1, "aucune traîne périmée");
    }

    #[test]
    fn params_follow_origin_rebase() {
        let mut trail = CarTrail::default();
        let p = DVec3::new(120.0, 2.0, -400_000.0);
        trail.record(p, 0.0);
        let before = trail.params(DVec3::new(0.0, 0.0, -399_000.0), 1.0)[0];
        let after = trail.params(DVec3::new(0.0, 0.0, -400_000.0), 1.0)[0];
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
