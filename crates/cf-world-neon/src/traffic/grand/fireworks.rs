//! Feu d'artifice (`events/fireworks.ts`) : des fusées montent derrière la ville et éclatent très haut, en coques, en
//! anneaux ou en palmes. Chaque étincelle suit une balistique simple (traînage, gravité) et s'éteint en scintillant. Trois
//! salves au plus à la fois ; un drop en déclenche trois d'un coup. Tout est en altitude et loin devant : rien n'approche de
//! la voiture.
//!
//! Écarts (voir la PR) : règle 7. L'éclat blanc de 70 m qui ouvrait chaque salve n'est plus un trait de lumière sec : la
//! salve **demande** un éclat au limiteur (`SceneFx::burst_request`) et le point n'a que l'éclat accordé (montée d'au moins
//! `MIN_FADE`, au plus trois départs par seconde, rien quand les flashs sont coupés). De même, les étincelles ne scintillent
//! (`twinkle` de Sillage) que si les flashs sont permis ; sinon elles brillent de façon régulière.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_render::SceneColor;

use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::{index, pick_tone, range};

const LIFE_S: f32 = 20.0;
const SPARKS: usize = 38;
const MAX_BURSTS: usize = 3;
/// Un drop lance trois salves d'un coup, même quand il y en a déjà : de la place pour la file de salves en vol.
const BURST_SLOTS: usize = 9;
const GRAVITY: f32 = 15.0;
const DRAG: f32 = 1.7;
const BURST_LIFE_S: f32 = 2.8;
const LAUNCH_S: f64 = 1.0;
/// L'éclat d'ouverture n'existe que dans la première demi-seconde de la salve (le limiteur l'éteint en moins).
const FLASH_WINDOW_S: f64 = 0.5;

#[derive(Debug, Clone, Copy)]
struct Burst {
    born: f64,
    origin: DVec3,
    /// Direction unitaire de chaque étincelle.
    dir: [Vec3; SPARKS],
    /// Vitesse de chaque étincelle, m/s.
    speed: [f32; SPARKS],
    tint: LinearRgba,
    tint2: LinearRgba,
    /// L'éclat d'ouverture a déjà été demandé au limiteur.
    announced: bool,
}

/// Une coque sphérique, un anneau incliné, ou une palme : trois façons de remplir le ciel.
fn shell(kind: usize, ctx: &mut EventCtx<'_>) -> ([Vec3; SPARKS], [f32; SPARKS]) {
    let mut dir = [Vec3::ZERO; SPARKS];
    let mut speed = [0.0; SPARKS];
    let tilt = range(ctx.rng, 0.0, std::f32::consts::PI);
    for i in 0..SPARKS {
        let k = i as f32 / SPARKS as f32;
        let (v, s) = match kind {
            0 => {
                let y = 1.0 - i as f32 / (SPARKS - 1) as f32 * 2.0;
                let r = (1.0 - y * y).sqrt();
                let a = i as f32 * 2.399_963;
                (Vec3::new(a.cos() * r, y, a.sin() * r), range(ctx.rng, 62.0, 92.0))
            }
            1 => {
                let a = k * std::f32::consts::TAU;
                (
                    Vec3::new(a.cos(), a.sin() * tilt.cos(), a.sin() * tilt.sin()),
                    range(ctx.rng, 82.0, 90.0),
                )
            }
            _ => {
                let a = (i % 8) as f32 * std::f32::consts::FRAC_PI_4 + range(ctx.rng, -0.1, 0.1);
                let lift = 0.4 + k * 0.6;
                (
                    Vec3::new(a.cos() * lift, 0.7 + k * 0.3, a.sin() * lift),
                    range(ctx.rng, 54.0, 104.0),
                )
            }
        };
        dir[i] = v.normalize();
        speed[i] = s;
    }
    (dir, speed)
}

#[derive(Debug)]
pub(crate) struct FireworksEvent {
    age: f32,
    wait: f32,
    bursts: Vec<Burst>,
    /// Dernière salve lancée : les plans du mode cinéma la filment en s'élevant vers elle.
    latest: DVec3,
    sun: LinearRgba,
    ice: LinearRgba,
    accents: [LinearRgba; 4],
}

impl FireworksEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let named = |c| ctx.frame.named(c);
        let (sun, ice) = (named(SceneColor::Sun), named(SceneColor::Ice));
        let accents = [sun, ice, named(SceneColor::Rose), named(SceneColor::Amber)];
        Self {
            age: 0.0,
            wait: range(ctx.rng, 0.2, 0.7),
            bursts: Vec::with_capacity(BURST_SLOTS),
            latest: DVec3::new(0.0, 140.0, 1.0e6),
            sun,
            ice,
            accents,
        }
    }

    fn launch(&mut self, ctx: &mut EventCtx<'_>) {
        let (dir, speed) = shell(index(ctx.rng, 3), ctx);
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let origin = DVec3::new(
            f64::from(side * range(ctx.rng, 15.0, 120.0)) + ctx.frame.car.x * 0.3,
            f64::from(range(ctx.rng, 110.0, 170.0)),
            ctx.frame.car.z - f64::from(range(ctx.rng, 190.0, 340.0)),
        );
        self.latest = origin;
        let tint = pick_tone(ctx);
        let tint2 = self.accents[index(ctx.rng, self.accents.len())];
        if self.bursts.len() == BURST_SLOTS {
            self.bursts.remove(0);
        }
        self.bursts.push(Burst {
            born: ctx.frame.time,
            origin,
            dir,
            speed,
            tint,
            tint2,
            announced: false,
        });
    }
}

impl TrafficEvent for FireworksEvent {
    /// Centre de la dernière salve et son ampleur. Avant la première, un point derrière la voiture, que les plans écartent.
    fn focus(&self) -> Option<Focus> {
        Some(Focus {
            position: self.latest,
            size: 90.0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        // La fin laisse éteindre les dernières salves.
        let finishing = self.age > LIFE_S;
        if finishing && self.bursts.is_empty() {
            return false;
        }
        self.wait -= dt * (0.8 + ctx.frame.drive.intensity);
        if finishing {
            self.wait = 1.0;
        } else if ctx.frame.drive.drop {
            for _ in 0..3 {
                self.launch(ctx);
            }
            self.wait = range(ctx.rng, 1.0, 1.8);
        } else if self.wait <= 0.0 && self.bursts.len() < MAX_BURSTS {
            self.launch(ctx);
            self.wait = range(ctx.rng, 0.7, 1.7);
        }

        let time = ctx.frame.time;
        self.bursts
            .retain(|b| time - b.born <= LAUNCH_S + f64::from(BURST_LIFE_S));
        let granted = ctx.scene.fx.burst;
        let permitted = ctx.scene.fx.flashes;
        for burst in &mut self.bursts {
            let t = time - burst.born;
            if t < LAUNCH_S {
                // Fusée : monte en ligne droite depuis le haut des tours, laissant une queue.
                let k = (t / LAUNCH_S) as f32;
                let y = 40.0 + (burst.origin.y as f32 - 40.0) * (k * (2.0 - k));
                for tail in 0..4 {
                    let tail_f = tail as f32;
                    ctx.glints.add(
                        DVec3::new(burst.origin.x, f64::from(y - tail_f * 4.0), burst.origin.z),
                        4.2 - tail_f * 0.8,
                        self.sun,
                        2.4 - tail_f * 0.4,
                    );
                }
                continue;
            }
            let s = (t - LAUNCH_S) as f32;
            // Au début de l'éclatement : un éclat, demandé au limiteur (une seule fois par salve).
            if !burst.announced {
                burst.announced = true;
                ctx.scene.fx.burst_request = true;
            }
            if f64::from(s) < FLASH_WINDOW_S && granted > 0.001 {
                ctx.glints.add(burst.origin, 70.0, self.ice, 3.2 * granted);
            }
            let fade = (1.0 - s / BURST_LIFE_S).max(0.0).powf(1.4);
            let drag = (1.0 - (-DRAG * s).exp()) / DRAG;
            for i in 0..SPARKS {
                let d = burst.dir[i];
                let v = burst.speed[i];
                let at = DVec3::new(
                    burst.origin.x + f64::from(d.x * v * drag),
                    burst.origin.y + f64::from(d.y * v * drag - 0.5 * GRAVITY * s * s),
                    burst.origin.z + f64::from(d.z * v * drag),
                );
                // Le scintillement n'existe que si les flashs sont permis ; chaque étincelle a sa phase.
                let twinkle = if permitted {
                    0.55 + 0.45 * (time as f32 * 22.0 + i as f32 * 3.7).sin()
                } else {
                    0.85
                };
                let tint = if i % 3 == 0 { burst.tint2 } else { burst.tint };
                ctx.glints.add(at, 13.0 * fade + 2.0, tint, 3.6 * fade * twinkle);
            }
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.bursts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;
    use crate::traffic::glints::GLINT_CAPACITY;

    #[test]
    fn chaque_salve_demande_son_eclat_une_seule_fois_et_n_en_a_pas_sans_limiteur() {
        let mut rig = Rig::new(8);
        let mut event = rig.start(FireworksEvent::new);
        let (mut requests, mut launched) = (0, 0);
        let mut previous = 0;
        for _ in 0..(30 * 60) {
            let alive = rig.step(&mut event, 1.0 / 60.0, |_| {});
            requests += usize::from(rig.scene.fx.burst_request);
            // Aucun limiteur ne répond dans ce banc : aucun éclat blanc de 70 m ne doit sortir.
            assert!(
                rig.glints.points().iter().all(|g| (g.size - 70.0).abs() > 1e-3),
                "éclat hors limiteur"
            );
            assert!(rig.glints.used() <= GLINT_CAPACITY);
            if event.bursts.len() > previous {
                launched += event.bursts.len() - previous;
            }
            previous = event.bursts.len();
            if !alive {
                break;
            }
        }
        assert!(launched >= 5, "des salves partent : {launched}");
        assert!(requests >= 4, "chaque salve demande un éclat : {requests}");
    }

    #[test]
    fn l_eclat_accorde_est_le_seul_qui_s_affiche() {
        let mut rig = Rig::new(8);
        let mut event = rig.start(FireworksEvent::new);
        let mut flashed = false;
        for _ in 0..(10 * 60) {
            rig.scene.fx.burst = 0.5;
            rig.step(&mut event, 1.0 / 60.0, |_| {});
            // `step` ne touche pas aux niveaux accordés : ils restent ceux du limiteur (ici posés à la main).
            for g in rig.glints.points() {
                if (g.size - 70.0).abs() < 1e-3 {
                    flashed = true;
                    // La palette du banc est blanche : l'éclat est exactement le niveau accordé fois 3,2.
                    assert!(
                        (g.color[0] - 3.2 * 0.5).abs() < 1e-3,
                        "éclat = niveau accordé : {}",
                        g.color[0]
                    );
                }
            }
        }
        assert!(flashed, "l'éclat accordé s'affiche");
    }

    #[test]
    fn un_drop_lance_trois_salves_et_la_file_ne_deborde_pas() {
        let mut rig = Rig::new(3);
        let mut event = rig.start(FireworksEvent::new);
        for _ in 0..(5 * 60) {
            rig.step(&mut event, 1.0 / 60.0, |frame| frame.drive.drop = true);
            assert!(event.bursts.len() <= BURST_SLOTS);
        }
        assert!(event.bursts.len() >= 3);
    }
}
