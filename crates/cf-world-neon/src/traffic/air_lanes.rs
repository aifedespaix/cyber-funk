//! Couloirs aériens au-dessus de la route (`air-lanes.ts`) : le trafic de fond, qui ne s'arrête jamais.
//!
//! La cadence suit l'énergie de la musique (basses et médiums) plutôt qu'un métronome, avec de l'aléa partout :
//! couloir, modèle, teinte, vitesse, et de temps en temps un gros porteur. La qualité adaptative l'espace quand le
//! processeur ou le GPU manquent (`TrafficFrame::density`).

use bevy::math::{DVec3, Vec3};

use super::event::EventCtx;
use super::fleet::CraftId;
use super::frame::FLOOR_MARGIN_M;
use super::motion::{Motion, Weave};
use super::variant::HEAVY;
use crate::pilot::rand::Rand;
use bevy::color::LinearRgba;

/// Un couloir : `dir` = −1, même sens que la voiture (ils la dépassent par l'arrière) ; 1, en sens inverse (ils
/// viennent de face). Un sens par couloir : deux véhicules ne se traversent pas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lane {
    /// Abscisse, m.
    pub x: f32,
    /// Altitude, m.
    pub y: f32,
    /// Sens (−1 : comme la voiture, 1 : à contresens).
    pub dir: i8,
}

/// Les couloirs sont posés sur des étages de vol (voir `route::floors`) : la voiture sait ainsi lesquels éviter.
pub const LANES: [Lane; 4] = [
    Lane { x: -6.5, y: 15.0, dir: -1 },
    Lane { x: 6.0, y: 21.0, dir: 1 },
    Lane { x: -1.5, y: 29.5, dir: -1 },
    Lane { x: 4.0, y: 38.5, dir: 1 },
];

/// Distance (m) devant la voiture où naît un véhicule à contresens, et derrière elle pour les autres (`AHEAD`, `BEHIND`).
const AHEAD_M: f64 = 520.0;
const BEHIND_M: f64 = 190.0;

/// Tire dans [min, max).
pub(crate) fn range(rng: &mut Rand, min: f32, max: f32) -> f32 {
    min + rng.unit() as f32 * (max - min)
}

/// Choisit un élément (`pick` de `context.ts`).
pub(crate) fn pick<'a, T>(rng: &mut Rand, items: &'a [T]) -> &'a T {
    &items[((rng.unit() * items.len() as f64) as usize).min(items.len() - 1)]
}

/// Place un véhicule au bout de la ligne de départ du couloir, vitesse en m/s dans le monde (`launchInLane`).
pub fn launch_in_lane(
    ctx: &mut EventCtx<'_>,
    lane: Lane,
    variant: u8,
    speed: f32,
    tint: LinearRgba,
    scale: f32,
) -> Option<CraftId> {
    let car = ctx.frame.car;
    let z = if lane.dir < 0 {
        car.z + BEHIND_M + f64::from(range(ctx.rng, 0.0, 40.0))
    } else {
        car.z - AHEAD_M - f64::from(range(ctx.rng, 0.0, 40.0))
    };
    let origin = DVec3::new(
        f64::from(lane.x + range(ctx.rng, -1.4, 1.4)),
        f64::from(lane.y + range(ctx.rng, -1.0, 1.0)),
        z,
    );
    let velocity = Vec3::new(0.0, 0.0, f32::from(lane.dir) * speed);
    let id = ctx.fleet.spawn(variant, origin, velocity, tint, scale, ctx.rng)?;
    let (ax, ay) = (range(ctx.rng, 0.2, 1.4), range(ctx.rng, 0.1, 0.8));
    let weave = Weave::random(ctx.rng, ax, ay);
    if let Some(craft) = ctx.fleet.get_mut(id) {
        craft.motion = Motion::path(origin, velocity, Some(weave), 0.0);
    }
    Some(id)
}

/// Le trafic de fond.
#[derive(Debug, Clone)]
pub struct AirLanes {
    countdown: f32,
    last_use: [f64; LANES.len()],
}

impl Default for AirLanes {
    fn default() -> Self {
        Self {
            countdown: 1.0,
            last_use: [-10.0; LANES.len()],
        }
    }
}

impl AirLanes {
    /// Avance d'une image : lance, de temps en temps, un véhicule dans un couloir libre.
    pub fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) {
        self.countdown -= dt;
        if self.countdown > 0.0 {
            return;
        }
        let frame = *ctx.frame;
        let energy = (frame.beat.bass * 0.6 + frame.beat.mid * 0.6).min(1.0);
        let base = if frame.calm { 5.5 } else { 2.6 - energy * 1.3 };
        // Une qualité dégradée espace les naissances : moins de véhicules à animer, à dessiner et à filmer.
        let density = frame.density.clamp(0.2, 1.0);
        self.countdown = (base * range(ctx.rng, 0.55, 1.45)).max(0.6) / density;

        // Jamais sur l'étage de la voiture (ni celui qu'elle vise) : on tire parmi les couloirs libres.
        let mut free = [0_usize; LANES.len()];
        let mut n = 0;
        for (i, lane) in LANES.iter().enumerate() {
            if frame.is_free(lane.y, FLOOR_MARGIN_M) {
                free[n] = i;
                n += 1;
            }
        }
        if n == 0 {
            return;
        }
        let index = *pick(ctx.rng, &free[..n]);
        if frame.time - self.last_use[index] < 1.2 {
            return;
        }
        self.last_use[index] = frame.time;

        let heavy = ctx.rng.unit() < 0.09;
        let variant = if heavy {
            *pick(ctx.rng, &HEAVY)
        } else {
            super::fleet::Fleet::sleek(ctx.rng.unit())
        };
        let speed = if heavy {
            range(ctx.rng, 30.0, 48.0)
        } else {
            range(ctx.rng, 58.0, 118.0)
        } * if frame.calm { 0.45 } else { 1.0 };
        let tint = *pick(ctx.rng, &frame.cycle);
        let scale = if heavy { 2.8 } else { range(ctx.rng, 1.7, 2.2) };
        launch_in_lane(ctx, LANES[index], variant, speed, tint, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::fleet::Fleet;
    use crate::traffic::frame::TrafficFrame;
    use crate::traffic::glints::Glints;
    use crate::traffic::walls::LightWalls;

    struct Rig {
        fleet: Fleet,
        glints: Glints,
        walls: LightWalls,
        rng: Rand,
        lanes: AirLanes,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                fleet: Fleet::new(),
                glints: Glints::new(),
                walls: LightWalls::new(),
                rng: Rand::new(11),
                lanes: AirLanes::default(),
            }
        }

        fn run(&mut self, frame: &TrafficFrame<'_>, seconds: f32) {
            let mut t = 0.0;
            let mut time = frame.time;
            while t < seconds {
                let f = TrafficFrame { time, ..*frame };
                let mut ctx = EventCtx {
                    frame: &f,
                    fleet: &mut self.fleet,
                    glints: &mut self.glints,
                    walls: &mut self.walls,
                    rng: &mut self.rng,
                };
                self.lanes.update(f.dt, &mut ctx);
                self.fleet.update(f.dt, f.car.z, |_| true);
                t += f.dt;
                time += f64::from(f.dt);
            }
        }
    }

    #[test]
    fn le_trafic_de_fond_naît_et_ne_s_arrete_pas() {
        let mut rig = Rig::new();
        rig.run(&TrafficFrame::default(), 30.0);
        assert!(rig.fleet.stats().stepped_total > 0);
        let mut seen = 0;
        rig.fleet.for_each_active(|_| seen += 1);
        assert!(seen > 3, "{seen} véhicules après 30 s");
    }

    #[test]
    fn les_vehicules_naissent_au_bout_de_la_ligne_et_dans_le_bon_sens() {
        let mut rig = Rig::new();
        rig.run(&TrafficFrame::default(), 6.0);
        rig.fleet.for_each_active(|c| {
            // Contresens : vitesse vers −z (positive en dir 1 → −? la vitesse z = dir × v).
            let lane = LANES.iter().find(|l| (l.y - c.position.y as f32).abs() < 3.0).expect("couloir");
            assert_eq!(c.velocity.z.signum() as i8, lane.dir);
        });
    }

    #[test]
    fn jamais_sur_l_etage_de_la_voiture() {
        let mut rig = Rig::new();
        let frame = TrafficFrame {
            altitude: 15.0,
            target: 15.0,
            ..TrafficFrame::default()
        };
        rig.run(&frame, 40.0);
        rig.fleet.for_each_active(|c| {
            assert!((c.position.y as f32 - 15.0).abs() > 6.5 - 1.5, "y = {}", c.position.y);
        });
    }

    #[test]
    fn une_qualite_degradee_espace_les_naissances() {
        let count = |density: f32| {
            let mut rig = Rig::new();
            rig.run(&TrafficFrame { density, ..TrafficFrame::default() }, 120.0);
            rig.fleet.stats().stepped_total
        };
        assert!(count(0.4) < count(1.0));
    }
}
