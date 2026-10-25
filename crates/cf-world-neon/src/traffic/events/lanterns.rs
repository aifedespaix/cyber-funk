//! Pluie de lanternes (`events/lanterns.ts`) : de petites lanternes chaudes montent du fond de la ville, entre les tours,
//! par centaines de mètres. Elles dérivent à peine et vacillent. Elles restent de part et d'autre de la route (à plus de
//! 10 m de l'axe) : ce ne sont que des lueurs, que la voiture, même volante, traverse sans heurt.

use bevy::color::LinearRgba;
use bevy::math::DVec3;

use super::common::{envelope, index, range, tone};
use crate::traffic::event::{EventCtx, TrafficEvent};

const LIFE_S: f32 = 28.0;
const COUNT: usize = 64;
const CEILING_M: f32 = 135.0;

#[derive(Debug, Clone, Copy)]
struct Lantern {
    x: f32,
    y: f32,
    z: f64,
    rise: f32,
    phase: f32,
    size: f32,
    tint: LinearRgba,
}

/// Une lanterne par case de [`COUNT`] ; le tableau est rempli une fois au départ, puis recyclé sur place.
#[derive(Debug)]
pub(crate) struct LanternEvent {
    age: f32,
    lanterns: Vec<Lantern>,
    tints: [LinearRgba; 4],
}

impl LanternEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let tints = [tone(ctx, 0), tone(ctx, 1), tone(ctx, 2), tone(ctx, 0)];
        let mut lanterns = Vec::with_capacity(COUNT);
        for _ in 0..COUNT {
            lanterns.push(spawn(ctx, true, &tints));
        }
        Self {
            age: 0.0,
            lanterns,
            tints,
        }
    }
}

fn spawn(ctx: &mut EventCtx<'_>, anywhere: bool, tints: &[LinearRgba; 4]) -> Lantern {
    let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
    let k = index(ctx.rng, tints.len());
    let car_z = ctx.frame.car.z;
    Lantern {
        x: side * range(ctx.rng, 11.0, 85.0),
        y: if anywhere {
            range(ctx.rng, 4.0, CEILING_M)
        } else {
            range(ctx.rng, 2.0, 10.0)
        },
        z: car_z - f64::from(range(ctx.rng, 40.0, 330.0)),
        rise: range(ctx.rng, 4.0, 9.0),
        phase: range(ctx.rng, 0.0, std::f32::consts::TAU),
        size: range(ctx.rng, 6.0, 9.5),
        tint: tints[k],
    }
}

impl TrafficEvent for LanternEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > LIFE_S {
            return false;
        }
        let envelope = envelope(self.age, LIFE_S, 3.0);
        let time = ctx.frame.time as f32;
        let rise_gain = 1.0 + ctx.frame.drive.intensity * 0.5;
        let mid = ctx.frame.beat.mid;
        let car = ctx.frame.car;
        for lantern in &mut self.lanterns {
            lantern.y += lantern.rise * dt * rise_gain;
            // Une lanterne qui a passé le plafond, ou que la voiture a dépassée de loin, repart d'en bas devant.
            if lantern.y > CEILING_M || lantern.z > car.z + 70.0 {
                *lantern = spawn(ctx, false, &self.tints);
            }
            let sway = (time * 0.7 + lantern.phase).sin() * 2.2;
            // Vacillement lent (moins de 1 Hz) : la variation reste loin d'un flash.
            let flicker = 0.75 + 0.25 * (time * 5.0 + lantern.phase * 3.0).sin() + mid * 0.5;
            let fade = (lantern.y / 14.0).min(1.0) * ((CEILING_M - lantern.y) / 25.0).min(1.0);
            ctx.glints.add(
                DVec3::new(f64::from(lantern.x + sway), f64::from(lantern.y), lantern.z),
                lantern.size,
                lantern.tint,
                2.6 * flicker * fade * envelope,
            );
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.lanterns.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pilot::rand::Rand;
    use crate::traffic::fleet::Fleet;
    use crate::traffic::frame::TrafficFrame;
    use crate::traffic::glints::Glints;
    use crate::traffic::walls::LightWalls;

    #[test]
    fn les_lanternes_restent_au_bord_de_la_route_et_s_eteignent() {
        let frame = TrafficFrame::default();
        let (mut fleet, mut glints, mut walls, mut rng) =
            (Fleet::new(), Glints::new(), LightWalls::new(), Rand::new(3));
        let mut ctx = EventCtx {
            frame: &frame,
            fleet: &mut fleet,
            glints: &mut glints,
            walls: &mut walls,
            rng: &mut rng,
        };
        let mut event = LanternEvent::new(&mut ctx);
        assert!(event.update(0.1, &mut ctx));
        assert_eq!(ctx.glints.used(), COUNT);
        assert!(event.lanterns.iter().all(|l| (11.0..=85.0).contains(&l.x.abs())));
        assert!(!event.update(LIFE_S, &mut ctx));
        event.end(&mut ctx);
        assert!(event.lanterns.is_empty());
    }
}
