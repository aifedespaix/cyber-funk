//! Nuée de drones lumineux (`events/swarm.ts`) : une cinquantaine de points qui se déplacent en formation au-dessus de la
//! route et passent d'une figure à l'autre (anneau, serpent, sphère, hélice), nerveux au tempo, jamais deux fois pareil.
//!
//! Les drones sont des points lumineux (`Glints`) : aucun véhicule, aucune entité. Leur nervosité au tempo est le pouls
//! que le limiteur laisse passer (`SceneFx::kick`) ; chaque drone vacille à sa propre phase, jamais tous ensemble.

use std::f32::consts::{PI, TAU};

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use crate::pilot::flight::Band;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::events::common::{index, range, smooth};

const DRONES: usize = 56;
const LIFE_S: f32 = 22.0;
/// La nuée n'est « proche de la route » que dans cette fenêtre devant et derrière la voiture, m.
const NEAR_AHEAD_M: f64 = 90.0;
const NEAR_BEHIND_M: f64 = 260.0;
/// Demi-hauteur de la tranche d'altitude réservée, formations comprises, m.
const CLAIM_M: f64 = 14.0;

type Shape = fn(usize, usize) -> Vec3;

/// Anneau, dans le plan de la route.
fn ring(i: usize, n: usize) -> Vec3 {
    let a = i as f32 / n as f32 * TAU;
    Vec3::new(a.cos() * 13.0, a.sin() * 9.0, 0.0)
}

/// Serpent étiré dans le sens de la marche.
fn serpent(i: usize, n: usize) -> Vec3 {
    let k = i as f32 / n as f32;
    Vec3::new((k * PI * 4.0).sin() * 8.0, (k * PI * 6.0).cos() * 4.0, (k - 0.5) * 70.0)
}

/// Sphère de Fibonacci.
fn sphere(i: usize, n: usize) -> Vec3 {
    let y = 1.0 - i as f32 / (n - 1) as f32 * 2.0;
    let r = (1.0 - y * y).sqrt();
    let a = i as f32 * 2.399_963;
    Vec3::new(a.cos() * r * 11.0, y * 11.0, a.sin() * r * 11.0)
}

/// Double hélice.
fn helix(i: usize, n: usize) -> Vec3 {
    let k = i as f32 / n as f32;
    let t = k * PI * 6.0;
    let arm = if i.is_multiple_of(2) { 1.0 } else { -1.0 };
    Vec3::new(t.cos() * 8.0 * arm, t.sin() * 8.0 * arm, (k - 0.5) * 60.0)
}

const SHAPES: [Shape; 4] = [ring, serpent, sphere, helix];

#[derive(Debug)]
pub(crate) struct SwarmEvent {
    age: f32,
    first: Shape,
    from: Shape,
    to: Shape,
    start: DVec3,
    drift: Vec3,
    spin: f32,
    hold: f32,
    centre: DVec3,
    tints: [LinearRgba; 6],
}

impl SwarmEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        // Trois figures distinctes parmi quatre : un tirage sans remise.
        let mut pool = SHAPES;
        for i in 0..3 {
            let j = i + index(ctx.rng, pool.len() - i);
            pool.swap(i, j);
        }
        let start = DVec3::new(
            f64::from(range(ctx.rng, -10.0, 10.0)),
            f64::from(range(ctx.rng, 34.0, 52.0)),
            ctx.frame.car.z - 380.0,
        );
        // Dérive à peine plus lente que la voiture : la nuée se rapproche doucement.
        let drift = Vec3::new(
            range(ctx.rng, -1.4, 1.4),
            0.0,
            -ctx.frame.speed * range(ctx.rng, 0.35, 0.6),
        );
        let spin = range(ctx.rng, 0.3, 0.9) * if ctx.rng.coin() { -1.0 } else { 1.0 };
        let hold = range(ctx.rng, 5.0, 8.0);
        Self {
            age: 0.0,
            first: pool[0],
            from: pool[1],
            to: pool[2],
            start,
            drift,
            spin,
            hold,
            centre: start,
            tints: ctx.frame.cycle,
        }
    }

    fn near(&self, ctx: &EventCtx<'_>) -> bool {
        let car_z = ctx.frame.car.z;
        self.centre.z < car_z + NEAR_AHEAD_M && self.centre.z > car_z - NEAR_BEHIND_M
    }
}

impl TrafficEvent for SwarmEvent {
    /// La nuée est proche de la route : sa tranche d'altitude (formations comprises) est réservée.
    fn proximity(&self, ctx: &EventCtx<'_>) -> bool {
        self.near(ctx)
    }

    fn claim(&self, ctx: &EventCtx<'_>) -> Option<Band> {
        self.near(ctx).then_some(Band {
            lo: self.centre.y - CLAIM_M,
            hi: self.centre.y + CLAIM_M,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > LIFE_S {
            return false;
        }
        let t = self.age;
        // Enveloppe : la nuée se « allume » puis s'éteint ; les formations s'enchaînent.
        let envelope = (t / 2.2).min(1.0) * ((LIFE_S - t) / 2.5).min(1.0);
        let morph1 = smooth(0.0, 1.0, (t - self.hold * 0.6) / 2.4);
        let morph2 = smooth(0.0, 1.0, (t - self.hold * 1.6) / 2.4);
        let mut centre = self.start + (self.drift * t).as_dvec3();
        centre.x += f64::from((t * 0.31).sin() * 9.0);
        centre.y += f64::from((t * 0.47 + 1.0).sin() * 4.0);
        self.centre = centre;
        let pulse = ctx.scene.fx.kick;
        let breathe = 1.0 + ctx.frame.beat.pulse * 0.16 + (t * 1.3).sin() * 0.05;
        let (sin, cos) = (t * self.spin).sin_cos();

        for i in 0..DRONES {
            let offset = (self.first)(i, DRONES)
                .lerp((self.from)(i, DRONES), morph1)
                .lerp((self.to)(i, DRONES), morph2);
            // Tourne autour de l'axe de marche, et chaque drone flotte autour de sa place.
            let rx = offset.x * cos - offset.y * sin;
            let ry = offset.x * sin + offset.y * cos;
            let wobble = t * 1.7 + i as f32 * 1.9;
            let at = DVec3::new(
                centre.x + f64::from(rx * breathe + wobble.sin() * 0.7),
                centre.y + f64::from(ry * breathe + (wobble * 1.3).cos() * 0.7),
                centre.z + f64::from(offset.z * breathe),
            );
            let flicker = 0.7 + 0.3 * (t * 6.0 + i as f32 * 2.1).sin() + pulse * 0.5;
            ctx.glints.add(
                at,
                1.5 * envelope,
                self.tints[i % self.tints.len()],
                2.0 * flicker * envelope,
            );
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn les_quatre_figures_tiennent_dans_la_nuee_et_restent_finies() {
        for shape in SHAPES {
            for i in 0..DRONES {
                let p = shape(i, DRONES);
                assert!(p.is_finite() && p.length() < 80.0, "{p:?}");
            }
        }
    }

    #[test]
    fn la_nuee_pose_un_point_par_drone_et_reserve_sa_tranche_quand_elle_est_proche() {
        let mut rig = Rig::new(5);
        let mut event = rig.start(SwarmEvent::new);
        let mut claimed = false;
        let mut seen = 0;
        for _ in 0..(LIFE_S as usize * 60) {
            if !rig.step(&mut event, 1.0 / 60.0, |_| {}) {
                break;
            }
            seen = seen.max(rig.glints.used());
            let frame = rig.frame();
            let mut scene = crate::traffic::scene::Scene::new();
            let ctx = EventCtx {
                frame: &frame,
                fleet: &mut rig.fleet,
                glints: &mut rig.glints,
                walls: &mut rig.walls,
                rng: &mut rig.rng,
                scene: &mut scene,
                signs: None,
            };
            if let Some(band) = event.claim(&ctx) {
                claimed = true;
                assert!(band.hi - band.lo > 20.0);
                assert!(event.proximity(&ctx));
            }
        }
        assert_eq!(seen, DRONES);
        assert!(claimed, "la nuée passe près de la voiture");
    }
}
