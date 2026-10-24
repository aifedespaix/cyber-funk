//! Fumée des pneus (`tires.ts`, classe `Smoke` et émission de `Tires.update`) : disques flous qui montent, s'élargissent
//! et s'éteignent, émis au démarrage appuyé (arrière), au freinage (avant) et sur la roue extérieure dans un virage
//! serré. Logique pure : un tampon circulaire de particules, aucune allocation en régime établi.
//!
//! Les traces au sol des pneus (`Skids`) ne sont pas reprises (voir la PR, « Écarts »).
//!
//! Les particules vivent dans le monde (elles restent là où elles sont nées quand la voiture avance) : leurs positions
//! sont gardées en `f32` relativement à un point absolu (`f64`), recalé quand la voiture s'en éloigne : précis à 400 km.

use bevy::color::{LinearRgba, Mix};
use bevy::math::{DVec3, Vec3};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

/// Nombre de particules (`SMOKE_COUNT`).
pub const SMOKE_COUNT: usize = 240;
/// Durée de vie moyenne, s (`SMOKE_LIFE`).
const SMOKE_LIFE_S: f32 = 1.3;
/// Écart des roues à l'axe, m (`WHEEL_X`).
const WHEEL_X: f32 = 0.85;
/// Roues arrière et avant, m devant ou derrière le centre (`REAR_Z`, `FRONT_Z`, la route va vers −Z).
const REAR_Z: f32 = 1.5;
const FRONT_Z: f32 = -1.4;
/// Au-delà de cette distance au point de référence, les positions sont recalées, m.
const REBASE_M: f64 = 500.0;
/// Sommets du maillage : un carré par particule.
pub const VERTICES: usize = SMOKE_COUNT * 4;

/// Les indices du maillage (constants : seuls les sommets bougent).
#[must_use]
pub fn indices() -> Vec<u32> {
    let mut out = Vec::with_capacity(SMOKE_COUNT * 6);
    for i in 0..SMOKE_COUNT as u32 {
        let v = i * 4;
        out.extend_from_slice(&[v, v + 1, v + 2, v + 1, v + 3, v + 2]);
    }
    out
}

/// Les UV des sommets (constants).
#[must_use]
pub fn uvs() -> Vec<[f32; 2]> {
    (0..SMOKE_COUNT)
        .flat_map(|_| [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]])
        .collect()
}

/// Ce que fait la conduite, 0 à 1 (`DriveFeel`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DriveFeel {
    /// Démarrage appuyé.
    pub launch: f32,
    /// Freinage.
    pub brake: f32,
    /// Glissade en virage.
    pub slide: f32,
    /// Sens de la glissade : +1 si la voiture dérive vers la droite.
    pub side: f32,
    /// La voiture roule-t-elle assez vite pour laisser de la fumée ?
    pub moving: f32,
}

impl DriveFeel {
    /// Rien : en l'air, ni fumée ni traces.
    pub const NONE: Self = Self {
        launch: 0.0,
        brake: 0.0,
        slide: 0.0,
        side: 0.0,
        moving: 0.0,
    };

    /// Le ressenti de la conduite d'après ce que la voiture fait (`sillage-scene.ts`) : un virage serré à vitesse de
    /// croisière fait glisser, une forte accélération fait patiner.
    #[must_use]
    pub fn measure(speed_mps: f32, speed_ratio: f32, lateral_mps: f32, accel_mps2: f32, brake: f32) -> Self {
        let unit = |x: f32| x.clamp(0.0, 1.0);
        Self {
            launch: unit((accel_mps2 - 8.0) / 18.0),
            brake,
            slide: unit((lateral_mps.abs() * speed_ratio.min(1.0) - 6.0) / 12.0),
            side: if lateral_mps == 0.0 { 0.0 } else { lateral_mps.signum() },
            moving: unit(speed_mps / 6.0),
        }
    }
}

/// Le tampon circulaire de particules.
#[derive(Debug, Clone)]
pub struct Smoke {
    base: DVec3,
    positions: [Vec3; SMOKE_COUNT],
    velocities: [Vec3; SMOKE_COUNT],
    /// Âge, 0 à 1 ; 1 ou plus : éteinte.
    ages: [f32; SMOKE_COUNT],
    lives_s: [f32; SMOKE_COUNT],
    sizes: [f32; SMOKE_COUNT],
    strengths: [f32; SMOKE_COUNT],
    cursor: usize,
    emit_debt: f32,
    rng: Pcg32,
}

impl Smoke {
    /// Une fumée vide ; `seed` fixe le hasard (même graine, mêmes particules).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            base: DVec3::ZERO,
            positions: [Vec3::ZERO; SMOKE_COUNT],
            velocities: [Vec3::ZERO; SMOKE_COUNT],
            ages: [1.0; SMOKE_COUNT],
            lives_s: [1.0; SMOKE_COUNT],
            sizes: [0.0; SMOKE_COUNT],
            strengths: [0.0; SMOKE_COUNT],
            cursor: 0,
            emit_debt: 0.0,
            rng: Pcg32::seed_from_u64(seed),
        }
    }

    /// Nombre de particules vivantes.
    #[must_use]
    pub fn alive(&self) -> usize {
        self.ages.iter().filter(|&&a| a < 1.0).count()
    }

    fn random(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Émet une particule en `at` (absolu, m), de vitesse `(vx, vz)` (la verticale est tirée).
    fn emit(&mut self, at: DVec3, vx: f32, vz: f32, strength: f32) {
        if (at - self.base).abs().max_element() > REBASE_M {
            let delta = (at - self.base).as_vec3();
            for p in &mut self.positions {
                *p -= delta;
            }
            self.base = at;
        }
        let i = self.cursor;
        self.cursor = (self.cursor + 1) % SMOKE_COUNT;
        let jitter = Vec3::new(self.random() - 0.5, 0.0, 0.0) * 0.3;
        self.positions[i] = (at - self.base).as_vec3() + jitter;
        self.velocities[i] = Vec3::new(
            vx + (self.random() - 0.5) * 1.6,
            0.5 + self.random() * 0.9,
            vz + (self.random() - 0.5) * 1.2,
        );
        self.ages[i] = 0.0;
        self.lives_s[i] = SMOKE_LIFE_S * (0.7 + self.random() * 0.5);
        self.sizes[i] = 0.9 + self.random() * 0.7;
        self.strengths[i] = strength;
    }

    /// Émet selon la conduite (`Tires.update`) : à l'arrière au démarrage, devant au freinage, sur la roue extérieure
    /// dans un virage serré. `car` : centre de la voiture au sol (absolu), `speed_mps` : sa vitesse, `density` : 0 à 1,
    /// réduit la fumée quand la scène manque de souffle.
    pub fn emit_for(&mut self, dt_s: f32, car: DVec3, speed_mps: f32, feel: &DriveFeel, density: f32) {
        let DriveFeel {
            launch,
            brake,
            slide,
            side,
            moving,
        } = *feel;
        self.emit_debt += dt_s * density * moving * (launch * 70.0 + brake * 60.0 + slide * 80.0);
        let drift = -speed_mps * 0.12;
        while self.emit_debt >= 1.0 {
            self.emit_debt -= 1.0;
            let wheel = if self.random() < 0.5 { -1.0 } else { 1.0 };
            let at = |dx: f32, y: f32, dz: f32| car + DVec3::new(f64::from(dx), f64::from(y), f64::from(dz));
            if slide > launch && slide > brake {
                // Roue extérieure : à l'opposé du sens de la glissade.
                let outer = -(if side == 0.0 { 1.0 } else { side.signum() });
                self.emit(at(outer * WHEEL_X, 0.15, REAR_Z), outer * 1.8, drift, 0.5 + slide * 0.5);
            } else if brake >= launch {
                self.emit(
                    at(wheel * WHEEL_X, 0.15, FRONT_Z),
                    wheel * 0.8,
                    drift,
                    0.45 + brake * 0.55,
                );
            } else {
                self.emit(
                    at(wheel * WHEEL_X * 0.8, 0.2, REAR_Z + 0.3),
                    wheel * 0.6,
                    drift - 1.5,
                    0.45 + launch * 0.55,
                );
            }
        }
    }

    /// Avance les particules d'une image.
    pub fn update(&mut self, dt_s: f32) {
        let drag = (1.0 - dt_s * 1.4).max(0.0);
        for i in 0..SMOKE_COUNT {
            if self.ages[i] >= 1.0 {
                continue;
            }
            self.ages[i] += dt_s / self.lives_s[i];
            self.velocities[i].x *= drag;
            self.velocities[i].z *= drag;
            self.positions[i] += self.velocities[i] * dt_s;
        }
    }

    /// Écrit les carrés tournés vers la caméra (`right`, `up` : ses axes) en repère rendu, et leurs couleurs RVBA
    /// (`rgb` : teinte × 0,9, `a` : opacité). Les particules éteintes sont réduites à un point, sans opacité. Retourne
    /// la boîte englobante des particules vivantes, pour l'`Aabb`.
    ///
    /// `positions` et `colors` doivent avoir [`VERTICES`] éléments.
    pub fn write(
        &self,
        origin: DVec3,
        (right, up): (Vec3, Vec3),
        tint: LinearRgba,
        positions: &mut [[f32; 3]],
        colors: &mut [[f32; 4]],
    ) -> Option<(Vec3, Vec3)> {
        debug_assert!(positions.len() >= VERTICES && colors.len() >= VERTICES);
        // Teinte de la fumée : celle de la voiture tirée de 45 % vers le blanc.
        let color = tint.mix(&LinearRgba::WHITE, 0.45);
        let anchor = (self.base - origin).as_vec3();
        let mut bounds: Option<(Vec3, Vec3)> = None;
        for i in 0..SMOKE_COUNT {
            let t = self.ages[i].clamp(0.0, 1.0);
            let alive = self.ages[i] < 1.0;
            let center = anchor + self.positions[i];
            let half = if alive {
                self.sizes[i] * (0.5 + t * 2.6) * 0.5
            } else {
                0.0
            };
            let alpha = if alive {
                (1.0 - t).powf(1.6) * smoothstep(0.0, 0.08, t) * self.strengths[i] * 0.5
            } else {
                0.0
            };
            let (r, u) = (right * half, up * half);
            let corners = [center - r - u, center + r - u, center - r + u, center + r + u];
            let rgba = [color.red * 0.9, color.green * 0.9, color.blue * 0.9, alpha];
            for (c, corner) in corners.iter().enumerate() {
                positions[i * 4 + c] = corner.to_array();
                colors[i * 4 + c] = rgba;
            }
            if alive {
                let (lo, hi) = bounds.unwrap_or((center, center));
                let reach = Vec3::splat(half * 1.5);
                bounds = Some((lo.min(center - reach), hi.max(center + reach)));
            }
        }
        bounds
    }
}

/// `smoothstep` de GLSL (`edge0` < `edge1`).
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn braking() -> DriveFeel {
        DriveFeel {
            brake: 1.0,
            moving: 1.0,
            ..DriveFeel::NONE
        }
    }

    fn run(smoke: &mut Smoke, seconds: f32, feel: &DriveFeel) {
        for _ in 0..(seconds * 60.0) as usize {
            smoke.emit_for(1.0 / 60.0, DVec3::new(0.0, 0.0, -1000.0), 40.0, feel, 1.0);
            smoke.update(1.0 / 60.0);
        }
    }

    #[test]
    fn rien_ne_sort_sans_conduite_ni_vitesse() {
        let mut smoke = Smoke::new(1);
        run(&mut smoke, 2.0, &DriveFeel::NONE);
        assert_eq!(smoke.alive(), 0);
        // Un freinage à l'arrêt ne fume pas non plus.
        run(
            &mut smoke,
            2.0,
            &DriveFeel {
                brake: 1.0,
                ..DriveFeel::NONE
            },
        );
        assert_eq!(smoke.alive(), 0);
    }

    #[test]
    fn le_freinage_fume_a_l_avant_et_la_fumee_s_eteint_apres_sa_vie() {
        let mut smoke = Smoke::new(1);
        run(&mut smoke, 0.5, &braking());
        // 60 particules par seconde × 0,5 s, plafonné par le tampon.
        assert!((20..=40).contains(&smoke.alive()), "{}", smoke.alive());
        // Les particules montent, et sortent des roues avant : devant le centre de la voiture (z = −1000 → < −1001).
        for (p, _) in smoke.positions.iter().zip(&smoke.ages).filter(|(_, a)| **a < 1.0) {
            assert!(p.y > 0.0, "la fumée monte : {p}");
            assert!(
                smoke.base.z + f64::from(p.z) < -1000.0 + f64::from(FRONT_Z) + 1.0,
                "devant la voiture : {p}"
            );
        }
        // Sans plus de freinage, tout s'éteint en moins de 2 s (vie max 1,3 × 1,2).
        run(&mut smoke, 2.0, &DriveFeel::NONE);
        assert_eq!(smoke.alive(), 0);
    }

    #[test]
    fn la_glissade_fume_sur_la_roue_exterieure() {
        let mut smoke = Smoke::new(3);
        // La voiture dérive vers la droite (+1) : la roue extérieure est la gauche (−x).
        let feel = DriveFeel {
            slide: 1.0,
            side: 1.0,
            moving: 1.0,
            ..DriveFeel::NONE
        };
        run(&mut smoke, 0.3, &feel);
        assert!(smoke.alive() > 5);
        for (p, _) in smoke.positions.iter().zip(&smoke.ages).filter(|(_, a)| **a < 1.0) {
            let x = smoke.base.x + f64::from(p.x);
            assert!(x < 0.0, "roue gauche (x absolu) : {x}");
        }
    }

    #[test]
    fn le_hasard_est_deterministe() {
        let (mut a, mut b) = (Smoke::new(42), Smoke::new(42));
        run(&mut a, 0.5, &braking());
        run(&mut b, 0.5, &braking());
        assert_eq!(a.positions, b.positions);
        let mut c = Smoke::new(43);
        run(&mut c, 0.5, &braking());
        assert_ne!(a.positions, c.positions);
    }

    #[test]
    fn le_recalage_garde_les_positions_absolues() {
        let mut smoke = Smoke::new(5);
        smoke.emit(DVec3::new(0.0, 0.0, -100.0), 0.0, 0.0, 1.0);
        let before = smoke.base + smoke.positions[0].as_dvec3();
        // Une émission à 2 km déclenche le recalage : la première particule ne bouge pas dans le monde.
        smoke.emit(DVec3::new(0.0, 0.0, -2100.0), 0.0, 0.0, 1.0);
        let after = smoke.base + smoke.positions[0].as_dvec3();
        assert!((before - after).length() < 1e-3, "{before} vs {after}");
        assert!(
            smoke.positions[1].length() < 1.0,
            "la nouvelle est près de la référence"
        );
    }

    #[test]
    fn les_disques_grossissent_puis_s_estompent() {
        let mut smoke = Smoke::new(9);
        smoke.emit(DVec3::new(0.0, 0.0, -50.0), 0.0, 0.0, 1.0);
        let mut positions = vec![[0.0; 3]; VERTICES];
        let mut colors = vec![[0.0; 4]; VERTICES];
        let axes = (Vec3::X, Vec3::Y);
        let width = |p: &[[f32; 3]]| p[1][0] - p[0][0];
        let tint = LinearRgba::rgb(0.14, 0.07, 0.54);
        smoke.write(DVec3::new(0.0, 0.0, -50.0), axes, tint, &mut positions, &mut colors);
        let young = (width(&positions), colors[0][3]);
        for _ in 0..40 {
            smoke.update(1.0 / 60.0);
        }
        let bounds = smoke.write(DVec3::new(0.0, 0.0, -50.0), axes, tint, &mut positions, &mut colors);
        assert!(bounds.is_some());
        assert!(
            width(&positions) > young.0 * 2.0,
            "s'élargit : {} → {}",
            young.0,
            width(&positions)
        );
        assert!(colors[0][3] < young.1 + 0.5 && colors[0][3] > 0.0);
        // Une particule éteinte est un point sans opacité.
        assert!(width(&positions[4..]).abs() < 1e-6 && colors[4][3] == 0.0);
    }

    #[test]
    fn la_conduite_se_mesure_comme_dans_sillage() {
        let calm = DriveFeel::measure(40.0, 1.0, 0.0, 0.0, 0.0);
        assert_eq!((calm.launch, calm.slide, calm.side), (0.0, 0.0, 0.0));
        assert!((calm.moving - 1.0).abs() < f32::EPSILON);
        let turning = DriveFeel::measure(40.0, 1.0, -12.0, 0.0, 0.0);
        assert!((turning.slide - 0.5).abs() < 1e-6 && turning.side < 0.0);
        assert!((DriveFeel::measure(40.0, 1.0, 0.0, 26.0, 0.0).launch - 1.0).abs() < 1e-6);
        assert!(DriveFeel::measure(3.0, 1.0, 0.0, 0.0, 0.0).moving < 0.6);
    }
}
