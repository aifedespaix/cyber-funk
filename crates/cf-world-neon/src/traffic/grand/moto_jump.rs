//! Saut de moto entre deux immeubles (X-10) : un gros immeuble borde la route d'un côté, une moto y roule sur le toit, saute
//! par-dessus la route et se réceptionne sur le toit de l'immeuble d'en face. La caméra reste sur la voiture : le saut se
//! passe au-dessus d'elle, on ne le filme pas pour lui-même.
//!
//! Quatre passages, tirés au hasard : à niveau, en descente, en montée, et un duo dont la seconde moto fait un tonneau.
//!
//! # Trajectoire
//!
//! Tout est calculé à la naissance, sans allocation par image. La moto avance à vitesse constante `vm` le long de la route
//! (plus lentement que la voiture, qui la rattrape) ; en travers, elle glisse vers l'arête, puis suit une parabole de durée
//! `T` qui relie l'arête de départ (hauteur `h0`) à celle d'arrivée (hauteur `h1`) : `vy = (h1 − h0)/T + g·T/2`, donc la
//! hauteur à l'arrivée est exactement `h1`. Le décollage est réglé pour que la moto soit encore à 70–95 m devant la voiture,
//! et le saut se déroule sous ses yeux.
//!
//! # Règles tenues
//!
//! - structure (deux immeubles, zone dégagée des deux côtés) : exclusive de ses semblables ;
//! - les immeubles sont des panneaux réécrits à chaque image, aucune entité ;
//! - les balises de toit battent au pouls que le limiteur laisse passer (`scene.fx.kick`), jamais au kick brut (règle 7).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use std::f32::consts::TAU;

use crate::scenery::Clearing;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::events::common::{alive, index, pick_tone, range, release, smooth, spawn_scripted};
use crate::traffic::fleet::CraftId;
use crate::traffic::model::bike::WHEEL;
use crate::traffic::panels::PanelKind;
use crate::traffic::variant::BIKE;

/// Façade côté route, m (comme la vitrine).
const FACE_M: f64 = 15.0;
/// Largeur de chaque immeuble, perpendiculairement à la route, m.
const WIDTH_M: f32 = 44.0;
/// Pesanteur, m/s².
const GRAVITY: f32 = 9.81;
/// Distance de la moto au départ de l'événement, m : la zone dégagée l'est déjà par les autres structures à cette portée.
const AHEAD_M: f64 = 340.0;
/// Marge de toit avant le premier et après le dernier passage de moto, m.
const ROOF_MARGIN_M: f64 = 30.0;
/// Le décollage et la réception se font à cette distance de l'arête, m.
const EDGE_M: f64 = 3.0;
/// Voie de roulage sur le toit, m depuis la façade.
const LANE_M: f64 = 12.0;
/// Durée du glissement vers l'arête avant le saut, et vers la voie après, s.
const ALIGN_S: f32 = 1.2;
/// Roulage après la réception, s (sert à dimensionner les toits).
const RUN_OUT_S: f32 = 2.5;
const SCALE: f32 = 1.3;
/// Le passage s'arrête de lui-même au bout de ce temps, s (voiture à l'arrêt).
const MAX_LIFE_S: f32 = 45.0;

/// Les quatre passages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Level,
    Drop,
    Climb,
    Pair,
}

const KINDS: [Kind; 4] = [Kind::Level, Kind::Drop, Kind::Climb, Kind::Pair];

/// Une moto lancée : son retard sur la première, son tonneau éventuel.
#[derive(Debug, Clone, Copy)]
struct Rider {
    craft: CraftId,
    delay_s: f32,
    roll: bool,
    tint: LinearRgba,
}

/// La parabole commune à toutes les motos du passage.
#[derive(Debug, Clone, Copy)]
struct Arc {
    /// Côté de départ : 1 droite, −1 gauche.
    side: f64,
    h0: f32,
    h1: f32,
    /// Durée du vol, s.
    air_s: f32,
    /// Vitesse le long de la route (vers −z), m/s.
    vm: f32,
    /// Instant du décollage, s.
    launch_s: f32,
    z_start: f64,
}

/// La pose de la moto à l'instant `t` de son propre temps.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    x: f64,
    y: f32,
    z: f64,
    /// Vitesse verticale, m/s.
    vy: f32,
    /// Part du vol écoulée, `0..1` (0 hors vol).
    flight: f32,
}

impl Arc {
    /// Abscisse du bord de décollage et du bord de réception.
    fn edges(&self) -> (f64, f64) {
        (self.side * (FACE_M + EDGE_M), -self.side * (FACE_M + EDGE_M + 3.0))
    }

    fn vy0(&self) -> f32 {
        (self.h1 - self.h0) / self.air_s + GRAVITY * self.air_s * 0.5
    }

    fn pose(&self, t: f32) -> Pose {
        let (x_launch, x_land) = self.edges();
        let lane0 = self.side * (FACE_M + LANE_M);
        let lane1 = -self.side * (FACE_M + LANE_M);
        let z = self.z_start - f64::from(self.vm * t);
        let tau = t - self.launch_s;
        if tau < 0.0 {
            // Roulage, puis glissement vers l'arête dans les dernières ALIGN_S.
            let k = f64::from(smooth(-ALIGN_S, 0.0, tau));
            return Pose {
                x: lane0 + (x_launch - lane0) * k,
                y: self.h0,
                z,
                vy: 0.0,
                flight: 0.0,
            };
        }
        if tau < self.air_s {
            let vx = (x_land - x_launch) / f64::from(self.air_s);
            let vy0 = self.vy0();
            return Pose {
                x: x_launch + vx * f64::from(tau),
                y: self.h0 + vy0 * tau - 0.5 * GRAVITY * tau * tau,
                z,
                vy: vy0 - GRAVITY * tau,
                flight: tau / self.air_s,
            };
        }
        let k = f64::from(smooth(0.0, ALIGN_S, tau - self.air_s));
        Pose {
            x: x_land + (lane1 - x_land) * k,
            y: self.h1,
            z,
            vy: 0.0,
            flight: 0.0,
        }
    }

    /// Fin du roulage prévu : la durée à partir de laquelle plus rien ne se passe sur les toits.
    fn end_s(&self) -> f32 {
        self.launch_s + self.air_s + RUN_OUT_S
    }
}

#[derive(Debug)]
pub(crate) struct MotoJumpEvent {
    arc: Arc,
    riders: Vec<Rider>,
    tint: LinearRgba,
    /// Étendue des toits (z absolu) : `z_far` est le plus négatif.
    z_far: f64,
    z_near: f64,
    time: f32,
}

impl MotoJumpEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let kind = KINDS[index(ctx.rng, KINDS.len())];
        let (h0, h1) = match kind {
            Kind::Level => {
                let h = range(ctx.rng, 26.0, 36.0);
                (h, h)
            }
            Kind::Drop => (range(ctx.rng, 38.0, 46.0), range(ctx.rng, 20.0, 26.0)),
            Kind::Climb => (range(ctx.rng, 20.0, 26.0), range(ctx.rng, 38.0, 46.0)),
            Kind::Pair => {
                let h = range(ctx.rng, 30.0, 38.0);
                (h, h)
            }
        };
        let speed = ctx.frame.speed.max(0.0);
        let vm = (speed * 0.35).max(14.0);
        let closing = speed - vm;
        let ahead_at_launch = range(ctx.rng, 70.0, 95.0);
        let launch_s = if closing > 8.0 {
            ((AHEAD_M as f32 - ahead_at_launch) / closing).clamp(5.0, 10.0)
        } else {
            7.0
        };
        let side = if ctx.rng.coin() { 1.0 } else { -1.0 };
        let arc = Arc {
            side,
            h0,
            h1,
            air_s: range(ctx.rng, 1.9, 2.3),
            vm,
            launch_s,
            z_start: ctx.frame.car.z - AHEAD_M,
        };
        let tint = pick_tone(ctx);
        let count = if kind == Kind::Pair { 2 } else { 1 };
        let mut riders = Vec::with_capacity(count);
        for i in 0..count {
            let delay_s = i as f32 * 0.55;
            let rider_tint = if i == 0 { tint } else { pick_tone(ctx) };
            let start = arc.pose(-delay_s);
            let position = DVec3::new(start.x, f64::from(start.y + WHEEL * SCALE), start.z);
            let velocity = Vec3::new(0.0, 0.0, -vm);
            let Some(craft) = spawn_scripted(ctx, BIKE, position, velocity, rider_tint, SCALE) else {
                continue;
            };
            if let Some(c) = ctx.fleet.get_mut(craft) {
                c.lean = 12.0;
                c.trail = 0.0;
            }
            riders.push(Rider {
                craft,
                delay_s,
                roll: kind == Kind::Pair && i == 1,
                tint: rider_tint,
            });
        }
        let last_delay = riders.last().map_or(0.0, |r| r.delay_s);
        Self {
            arc,
            riders,
            tint,
            z_near: arc.z_start + f64::from(vm * last_delay) + ROOF_MARGIN_M,
            z_far: arc.z_start - f64::from(vm * arc.end_s()) - ROOF_MARGIN_M,
            time: 0.0,
        }
    }

    /// Un toit : le pavé, et ses balises aux deux bouts.
    fn building(&self, ctx: &mut EventCtx<'_>, side: f64, height: f32, param: f32) {
        let centre_x = side * (FACE_M + f64::from(WIDTH_M) * 0.5);
        let zc = f64::midpoint(self.z_near, self.z_far);
        let span = (self.z_near - self.z_far) as f32;
        ctx.scene.panels.block(
            PanelKind::Body,
            PanelKind::Body,
            DVec3::new(centre_x, 0.0, zc),
            Vec3::new(WIDTH_M, height, span),
            false,
            self.tint,
            1.0,
            param,
        );
        // Le pouls que le limiteur laisse passer, jamais le kick brut.
        let pulse = ctx.scene.fx.kick;
        let edge_x = side * (FACE_M + 1.5);
        for z in [self.z_near - 4.0, self.z_far + 4.0] {
            ctx.glints.add(
                DVec3::new(edge_x, f64::from(height + 1.5), z),
                2.6,
                self.tint,
                1.0 + pulse * 1.2,
            );
        }
    }

    /// Libère les motos et rend vrai quand aucune n'est plus en jeu.
    fn finish(&mut self, ctx: &mut EventCtx<'_>) {
        for rider in self.riders.drain(..) {
            release(ctx, Some(rider.craft));
        }
    }
}

impl TrafficEvent for MotoJumpEvent {
    fn structure(&self) -> bool {
        true
    }

    fn clearing(&self) -> Option<Clearing> {
        Some(Clearing {
            z0: self.z_far,
            z1: self.z_near,
            side: 0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.time += dt;
        let car = ctx.frame.car;
        // La voiture a dépassé les toits (ou ne bouge plus depuis longtemps) : fin.
        if car.z < self.z_far - 60.0 || self.time > MAX_LIFE_S {
            return false;
        }
        self.building(ctx, self.arc.side, self.arc.h0, 0.31);
        self.building(ctx, -self.arc.side, self.arc.h1, 0.67);

        for rider in &mut self.riders {
            if !alive(ctx, Some(rider.craft)) {
                continue;
            }
            let t = self.time - rider.delay_s;
            let pose = self.arc.pose(t);
            let position = DVec3::new(pose.x, f64::from(pose.y + WHEEL * SCALE), pose.z);
            // Les motos ne sont plus utiles une fois la voiture passée, loin derrière elle : rendues au pool.
            let passed = t > self.arc.launch_s + self.arc.air_s && pose.z > car.z + 25.0;
            if passed {
                release(ctx, Some(rider.craft));
                rider.delay_s = f32::INFINITY;
                continue;
            }
            let Some(craft) = ctx.fleet.get_mut(rider.craft) else {
                continue;
            };
            let previous = craft.position;
            craft.position = position;
            craft.velocity = if dt > 1e-3 && t > 0.0 {
                ((position - previous) / f64::from(dt)).as_vec3()
            } else {
                Vec3::new(0.0, 0.0, -self.arc.vm)
            };
            // Nez levé à la montée, piqué à la descente ; le tonneau du duo se fait au milieu du vol.
            let target = if pose.flight > 0.0 {
                (pose.vy.atan2(self.arc.vm) * 0.7).clamp(-0.6, 0.6)
            } else {
                0.0
            };
            craft.pitch += (target - craft.pitch) * (1.0 - (-dt * 8.0).exp());
            craft.roll = if rider.roll {
                TAU * smooth(0.12, 0.88, pose.flight)
            } else {
                0.0
            };
            let tint = rider.tint;
            let glow = if pose.flight > 0.0 { 1.7 } else { 1.0 };
            // Traînée de points derrière la moto : trois points qui rétrécissent, sans mur (le mur est au niveau du sol).
            let behind = craft.velocity * 0.045;
            for k in 1..=3_u8 {
                let back = DVec3::from(behind) * f64::from(k);
                ctx.glints.add(
                    position - back + DVec3::new(0.0, 0.35, 0.0),
                    2.4 - 0.5 * f32::from(k),
                    tint,
                    glow,
                );
            }
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        self.finish(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    fn arc(side: f64, h0: f32, h1: f32) -> Arc {
        Arc {
            side,
            h0,
            h1,
            air_s: 2.0,
            vm: 20.0,
            launch_s: 6.0,
            z_start: -1000.0,
        }
    }

    /// La parabole part de l'arête de départ à `h0` et se pose sur l'arête d'arrivée, exactement à `h1`.
    #[test]
    fn la_parabole_relie_les_deux_toits() {
        for (h0, h1) in [(30.0, 30.0), (42.0, 22.0), (22.0, 42.0)] {
            let a = arc(1.0, h0, h1);
            let (x_launch, x_land) = a.edges();
            let start = a.pose(a.launch_s);
            let end = a.pose(a.launch_s + a.air_s);
            assert!(
                (start.y - h0).abs() < 1e-3 && (start.x - x_launch).abs() < 1e-6,
                "{start:?}"
            );
            assert!((end.y - h1).abs() < 1e-3 && (end.x - x_land).abs() < 1e-6, "{end:?}");
            // Au milieu du vol la moto est au-dessus de la route, entre les deux façades.
            let mid = a.pose(a.launch_s + a.air_s * 0.5);
            assert!(mid.x.abs() < FACE_M, "au-dessus de la route : {mid:?}");
            assert!(mid.y > h0.min(h1), "{mid:?}");
        }
    }

    /// La pose est continue (aucun saut d'une image à l'autre), de la naissance à la fin du roulage.
    #[test]
    fn la_pose_est_continue() {
        let a = arc(-1.0, 40.0, 24.0);
        let mut previous = a.pose(0.0);
        let mut t = 0.0;
        while t < a.end_s() {
            t += 1.0 / 60.0;
            let p = a.pose(t);
            let step = ((p.x - previous.x).powi(2) + f64::from(p.y - previous.y).powi(2)).sqrt();
            assert!(step < 1.0, "saut de {step} m à t = {t}");
            previous = p;
        }
    }

    #[test]
    fn la_moto_part_et_atterrit_de_cotes_opposes() {
        for side in [1.0, -1.0] {
            let a = arc(side, 30.0, 30.0);
            let (launch, land) = a.edges();
            assert!(launch * land < 0.0, "cotés opposés");
            assert!(launch * side > 0.0, "part du côté choisi");
        }
    }

    fn event(rig: &mut Rig) -> MotoJumpEvent {
        rig.start(MotoJumpEvent::new)
    }

    #[test]
    fn c_est_une_structure_qui_degage_les_deux_cotes() {
        let mut rig = Rig::new(3);
        let ev = event(&mut rig);
        assert!(ev.structure());
        let clearing = ev.clearing().expect("zone");
        assert_eq!(clearing.side, 0);
        assert!(clearing.z0 < clearing.z1 && clearing.z1 < 0.0);
    }

    #[test]
    fn toutes_les_motos_sont_rendues_et_les_toits_sont_dessines() {
        for seed in 0..12 {
            let mut rig = Rig::new(seed);
            let mut ev = event(&mut rig);
            let mut frames = 0;
            let mut peak_panels = 0;
            while rig.step(&mut ev, 1.0 / 60.0, |_| {}) {
                frames += 1;
                peak_panels = peak_panels.max(rig.scene.panels.used());
                assert!(frames < 60 * 60, "ne finit jamais (graine {seed})");
            }
            rig.end(&mut ev);
            assert_eq!(rig.active_crafts(), 0, "motos non rendues (graine {seed})");
            assert!(peak_panels >= 10, "deux pavés au moins : {peak_panels}");
        }
    }

    /// La moto décolle devant la voiture, pas derrière elle : le saut se voit.
    #[test]
    fn le_decollage_a_lieu_devant_la_voiture() {
        let mut rig = Rig::new(5);
        let mut ev = event(&mut rig);
        let mut launched_ahead = None;
        for _ in 0..(60 * 20) {
            if !rig.step(&mut ev, 1.0 / 60.0, |_| {}) {
                break;
            }
            let car_z = rig.frame().car.z;
            if ev.time >= ev.arc.launch_s && launched_ahead.is_none() {
                let rider = ev.riders[0];
                let z = rig.fleet.get(rider.craft).map(|c| c.position.z);
                launched_ahead = z.map(|z| car_z - z);
            }
        }
        // `car_z - z` : positif quand la moto est devant (z plus négatif).
        assert!(launched_ahead.is_some_and(|d| d > 30.0), "{launched_ahead:?}");
    }
}
