//! Orage électrique (`events/storm.ts`) : des éclairs frappent le sommet des tours de chaque côté de la route, au rythme
//! des kicks (plus nombreux et plus violents quand le morceau monte). Chaque coup est une ligne brisée de points lumineux
//! qui s'éteint en 0,4 s, double parfois, et blanchit les façades un instant. Les coups restent à 18 m au moins de l'axe :
//! la voiture, même volante, ne risque rien et peut les traverser des yeux sans que rien ne réagisse.
//!
//! Écarts (voir la PR) : le blanchissement des façades (`uFlash`) n'est plus calculé ici, c'est le niveau que le limiteur
//! de flashs accorde (règle 7) : l'événement **demande** un éclair à chaque coup (`SceneFx::lightning_request`) ; le tracé
//! lumineux du coup, lui, ne scintille plus (le vacillement à 6 Hz de Sillage est remplacé par une montée en 0,08 s et une
//! descente régulière).

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_render::SceneColor;

use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::events::common::range;

const LIFE_S: f32 = 22.0;
const SEGMENTS: usize = 11;
const BOLT_LIFE_S: f32 = 0.42;
const MAX_BOLTS: usize = 2;
/// Le tracé monte en lumière en 0,08 s (`MIN_FADE`) : jamais un allumage sec.
const BOLT_RISE_S: f32 = 0.08;
/// Un coup ne frappe jamais à moins de 18 m de l'axe de la route.
const MIN_X_M: f64 = 18.0;

#[derive(Debug, Clone, Copy)]
struct Bolt {
    born: f64,
    /// Points du tracé, du ciel vers le sol.
    path: [DVec3; SEGMENTS + 1],
    tint: LinearRgba,
}

#[derive(Debug)]
pub(crate) struct StormEvent {
    age: f32,
    bolts: Vec<Bolt>,
    wait: f32,
    white: LinearRgba,
    cold: LinearRgba,
}

impl StormEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        Self {
            age: 0.0,
            bolts: Vec::with_capacity(MAX_BOLTS),
            wait: range(ctx.rng, 0.6, 1.4),
            white: ctx.frame.named(SceneColor::Ice),
            cold: ctx.frame.named(SceneColor::Azure),
        }
    }

    fn strike(&mut self, ctx: &mut EventCtx<'_>) {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let mut x = f64::from(side * range(ctx.rng, 18.0, 75.0));
        let mut y = f64::from(range(ctx.rng, 210.0, 270.0));
        let mut z = ctx.frame.car.z - f64::from(range(ctx.rng, 70.0, 320.0));
        let ground_y = f64::from(range(ctx.rng, 25.0, 95.0));
        let mut path = [DVec3::new(x, y, z); SEGMENTS + 1];
        for (i, point) in path.iter_mut().enumerate().skip(1) {
            let t = i as f64 / SEGMENTS as f64;
            // Descend par à-coups, avec une dérive latérale qui s'amortit.
            y = 270.0 - (270.0 - ground_y) * t + f64::from(range(ctx.rng, -3.0, 3.0)) * (1.0 - t);
            x += f64::from(range(ctx.rng, -9.0, 9.0)) * (1.0 - t * 0.5);
            z += f64::from(range(ctx.rng, -9.0, 9.0)) * (1.0 - t * 0.5);
            // Jamais sur la chaussée : on repousse le tracé au-delà de 18 m de l'axe.
            if x.abs() < MIN_X_M {
                let sign = if x.abs() > 0.0 { x.signum() } else { f64::from(side) };
                x = sign * MIN_X_M;
            }
            *point = DVec3::new(x, y, z);
        }
        let tint = if ctx.rng.coin() { self.white } else { self.cold };
        self.bolts.push(Bolt {
            born: ctx.frame.time,
            path,
            tint,
        });
        // Le blanchissement de la ville est une demande au limiteur, pas un réglage.
        ctx.scene.fx.lightning_request = true;
    }
}

impl TrafficEvent for StormEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        let envelope = (self.age / 2.0).min(1.0) * ((LIFE_S - self.age) / 2.5).min(1.0);
        if self.age > LIFE_S {
            return false;
        }

        self.wait -= dt * (0.7 + ctx.frame.drive.intensity * 1.1);
        let drop = ctx.frame.drive.drop;
        let kick = ctx.frame.beat.kick && ctx.frame.beat.bass > 0.55 && ctx.rng.unit() < 0.55;
        if (self.wait <= 0.0 || kick || drop) && self.bolts.len() < MAX_BOLTS && envelope > 0.4 {
            self.strike(ctx);
            self.wait = range(ctx.rng, 1.1, 3.2);
            // Un second coup juste derrière le premier : le grondement d'un orage vrai.
            if ctx.rng.unit() < 0.3 {
                self.wait = 0.12;
            }
        }

        // Ce que le limiteur a accordé à l'image précédente : l'impact s'adoucit si l'éclair n'est pas permis.
        let granted = ctx.scene.fx.lightning;
        let time = ctx.frame.time;
        self.bolts.retain(|bolt| ((time - bolt.born) as f32) < BOLT_LIFE_S);
        for bolt in &self.bolts {
            let age = (time - bolt.born) as f32;
            let life = age / BOLT_LIFE_S;
            let fade = (1.0 - life) * (age / BOLT_RISE_S).min(1.0);
            let p = &bolt.path;
            // Chaque tronçon du tracé est rempli de points : un trait continu, plus large en haut.
            for k in 0..SEGMENTS {
                let along = k as f32 / SEGMENTS as f32;
                for sub in 0..3 {
                    let u = f64::from(sub) / 3.0;
                    ctx.glints.add(
                        p[k].lerp(p[k + 1], u),
                        9.0 + 7.0 * (1.0 - along),
                        bolt.tint,
                        3.4 * fade * envelope,
                    );
                }
            }
            // Gerbe d'impact au bout du tracé.
            ctx.glints.add(
                p[SEGMENTS],
                55.0,
                self.white,
                2.6 * fade * envelope * (0.35 + 0.65 * granted),
            );
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.bolts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;
    use crate::traffic::glints::GLINT_CAPACITY;

    #[test]
    fn les_coups_restent_loin_de_l_axe_et_demandent_un_eclair_au_limiteur() {
        let mut rig = Rig::new(21);
        let mut event = rig.start(StormEvent::new);
        let mut requests = 0;
        let mut strikes_seen = 0;
        for i in 0..(25 * 60) {
            let alive = rig.step(&mut event, 1.0 / 60.0, |frame| {
                frame.beat.kick = i % 28 == 0;
                frame.beat.bass = 0.8;
                frame.drive.intensity = 0.8;
            });
            requests += usize::from(rig.scene.fx.lightning_request);
            strikes_seen = strikes_seen.max(event.bolts.len());
            assert!(rig.glints.used() <= GLINT_CAPACITY);
            for bolt in &event.bolts {
                assert!(
                    bolt.path.iter().all(|p| p.x.abs() >= MIN_X_M - 1e-9),
                    "un coup sur la route"
                );
            }
            if !alive {
                break;
            }
        }
        assert!(requests >= 4, "l'orage frappe : {requests} coups");
        assert!(strikes_seen <= MAX_BOLTS);
        // La façade n'est pas réglée par l'événement : tant qu'aucun limiteur ne répond, rien ne blanchit.
        assert_eq!(rig.scene.fx.lightning, 0.0);
    }

    #[test]
    fn la_fin_vide_les_coups() {
        let mut rig = Rig::new(2);
        let mut event = rig.start(StormEvent::new);
        for _ in 0..600 {
            rig.step(&mut event, 1.0 / 60.0, |frame| frame.drive.drop = true);
        }
        assert!(!event.bolts.is_empty());
        rig.end(&mut event);
        assert!(event.bolts.is_empty());
    }
}
