//! Vaisseau publicitaire (`events/adship.ts`) : un cargo arrive par l'arrière, rattrape la voiture, passe au-dessus d'elle
//! et s'éloigne devant. Le vol n'est pas tracé : cap, vitesse, hauteur et dérive latérale sont des ressorts poussés par des
//! rafales (bruit déterministe). La coque, de jour comme de nuit, forme un volume que la voiture contourne.
//!
//! Écarts (voir la PR) :
//! - pas de panneaux pendus : [`EventCtx`] ne donne pas accès au gestionnaire d'enseignes (`SignLender`). Le vaisseau vole
//!   donc sans son palonnier, et l'obstacle est la seule coque (c'est le cas du « mode fantôme » de Sillage) ;
//! - le projecteur blanc qui suit la voiture est publié dans la scène (voir `airship`) ;
//! - le vaisseau part toujours, là où Sillage ne partait que si des panneaux étaient libres.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use super::common::{alive, index, pick_tone, pin, range, release, set_pose, smooth, spawn_scripted, tone};
use crate::pilot::flight::{Band, Obstruction};
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::CraftId;
use crate::traffic::variant::CARGO;

const SCALE: f32 = 8.0;
/// Le palonnier pend sous la coque : environ ce quart d'échelle plus un mètre.
const HULL: f32 = SCALE * 0.3 + 1.0;
const START_M: f32 = -190.0;
const END_M: f32 = 270.0;

/// Bruit déterministe lisse (`noise` de `adship.ts`) : somme de sinusoïdes de fréquences incommensurables.
fn noise(t: f32, p: f32) -> f32 {
    (t * 0.9 + p).sin() * 0.6 + (t * 2.3 + p * 1.7).sin() * 0.3 + (t * 0.37 + p * 0.5).sin() * 0.8
}

#[derive(Debug)]
pub(crate) struct AdShipEvent {
    craft: Option<CraftId>,
    alive: bool,
    side: f32,
    seed: f32,
    cruise: f32,
    lane: f32,
    altitude: f32,
    /// Distance devant la voiture, m (négative : derrière).
    r: f32,
    vr: f32,
    x: f32,
    vx: f32,
    y: f32,
    vy: f32,
    time: f32,
    ship: DVec3,
    /// Point visé par le projecteur blanc : il suit la voiture avec du retard.
    aim_x: f32,
    aim_z: f64,
    ice: LinearRgba,
    alarm: LinearRgba,
    mint: LinearRgba,
}

impl AdShipEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let cruise = (if ctx.frame.calm { 9.0 } else { 15.0 }) + range(ctx.rng, -1.5, 2.5);
        let lane = side * range(ctx.rng, 4.0, 8.0);
        let altitude = range(ctx.rng, 34.0, 40.0);
        let x = lane - side * 14.0;
        let y = altitude + range(ctx.rng, -3.0, 3.0);
        let seed = range(ctx.rng, 0.0, std::f32::consts::TAU);
        let velocity = Vec3::new(0.0, 0.0, -ctx.frame.speed - 12.0);
        let origin = DVec3::new(f64::from(x), f64::from(y), ctx.frame.car.z - f64::from(START_M));
        let tint = pick_tone(ctx);
        let variant = CARGO[index(ctx.rng, CARGO.len())];
        let craft = spawn_scripted(ctx, variant, origin, velocity, tint, SCALE);
        if let Some(c) = craft.and_then(|id| ctx.fleet.get_mut(id)) {
            c.trail = 0.0;
            c.lean = 3.0;
        }
        Self {
            craft,
            alive: craft.is_some(),
            side,
            seed,
            cruise,
            lane,
            altitude,
            r: START_M,
            vr: 12.0,
            x,
            vx: 0.0,
            y,
            vy: 0.0,
            time: 0.0,
            ship: origin,
            aim_x: ctx.frame.car.x as f32,
            aim_z: ctx.frame.car.z,
            ice: tone(ctx, 2),
            alarm: tone(ctx, 0),
            mint: tone(ctx, 3),
        }
    }
}

impl TrafficEvent for AdShipEvent {
    fn proximity(&self, _ctx: &EventCtx<'_>) -> bool {
        self.r > -80.0 && self.r < 230.0
    }

    /// La coque forme un volume qui balaie la route : la voiture le contourne par dessus ou par dessous.
    fn obstruction(&self) -> Option<Obstruction> {
        if !self.alive {
            return None;
        }
        let half = SCALE * 1.2;
        // Le vaisseau vient de l'arrière puis s'éloigne : on ne s'en soucie que tout près de la voiture.
        if self.r < -45.0 || self.r > half + 25.0 {
            return None;
        }
        Some(Obstruction {
            z_near: self.ship.z + f64::from(half),
            z_far: self.ship.z - f64::from(half),
            lo: self.ship.y - f64::from(HULL),
            hi: self.ship.y + f64::from(SCALE * 0.3 + 1.0),
        })
    }

    fn claim(&self, ctx: &EventCtx<'_>) -> Option<Band> {
        if !self.proximity(ctx) {
            return None;
        }
        // Coque au-dessus, palonnier (absent ici) et panneaux suspendus dessous : la tranche garde la place.
        Some(Band {
            lo: f64::from(self.y - 18.0),
            hi: f64::from(self.y + 7.0),
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        let Some(id) = self.craft else {
            return false;
        };
        if !alive(ctx, Some(id)) || self.r > END_M {
            self.alive = false;
            return false;
        }
        self.time += dt;
        let t = self.time + self.seed;

        // Vitesse relative : il flâne à hauteur de la voiture, pour qu'on ait le temps de lire.
        let abeam = 1.0 - smooth(25.0, 90.0, self.r.abs());
        let wanted_vr = (self.cruise + noise(t, 1.0) * 2.5) * (1.0 - 0.55 * abeam);
        self.vr += (wanted_vr - self.vr) * (dt * 0.8).min(1.0);
        self.r += self.vr * dt;

        // Dérive latérale et altitude : des ressorts mous, poussés par des rafales.
        let approach = ((self.r + 110.0).max(0.0) / 110.0).min(1.0);
        let wanted_x = self.lane - self.side * approach * 3.0 + noise(t, 2.2) * 2.4;
        self.vx += ((wanted_x - self.x) * 1.1 - self.vx * 1.7) * dt;
        self.x += self.vx * dt;
        let wanted_y = self.altitude + noise(t * 0.8, 4.1) * 1.6 + (t * 0.55).sin() * 0.8;
        self.vy += ((wanted_y - self.y) * 1.4 - self.vy * 1.9) * dt;
        self.y += self.vy * dt;

        let car_z = ctx.frame.car.z;
        self.ship = DVec3::new(f64::from(self.x), f64::from(self.y), car_z - f64::from(self.r));
        let velocity = Vec3::new(self.vx, self.vy, -ctx.frame.speed - self.vr);
        set_pose(ctx, id, self.ship, velocity);
        self.lights(ctx);
        self.spot(ctx, dt, t);
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        self.alive = false;
        release(ctx, self.craft.take());
    }
}

impl AdShipEvent {
    /// Le projecteur blanc cherche la voiture avec du retard, tremble un peu, et l'éclaire de loin comme de près.
    fn spot(&mut self, ctx: &mut EventCtx<'_>, dt: f32, t: f32) {
        if self.r.abs() >= 140.0 {
            return;
        }
        let follow = (dt * 2.2).min(1.0);
        let car = ctx.frame.car;
        self.aim_x += (car.x as f32 + (t * 1.9).sin() * 0.9 - self.aim_x) * follow;
        self.aim_z += (car.z - 2.0 + f64::from((t * 1.3).sin() * 3.5) - self.aim_z) * f64::from(follow);
        let k = 0.4 + 0.6 * smooth(140.0, 80.0, self.r.abs());
        let tint = LinearRgba::rgb(self.ice.red * k, self.ice.green * k, self.ice.blue * k);
        pin(
            ctx,
            self.ship - DVec3::new(0.0, f64::from(HULL), 0.0),
            self.aim_x,
            self.aim_z,
            tint,
        );
    }

    /// Feux de position et phare de ventre, comme le cargo des airs.
    fn lights(&self, ctx: &mut EventCtx<'_>) {
        let p = self.ship;
        let s = f64::from(SCALE);
        ctx.glints.add(
            p + DVec3::new(-s * 0.9, s * 0.1, s * 0.2),
            SCALE * 0.22,
            self.alarm,
            2.2,
        );
        ctx.glints
            .add(p + DVec3::new(s * 0.9, s * 0.1, s * 0.2), SCALE * 0.22, self.mint, 2.2);
        ctx.glints
            .add(p + DVec3::new(0.0, -s * 0.1, -s * 0.9), SCALE * 0.28, self.ice, 1.4);
    }
}
