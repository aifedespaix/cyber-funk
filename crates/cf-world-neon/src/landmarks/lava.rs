//! La façade « Lave de code » (`lava-building.ts`) : des 0 et des 1 en fusion qui coulent sur un magma, une flaque qui
//! déborde sur le trottoir, quatre lumières orange pour le reflet de la route mouillée.
//!
//! Écarts avec Sillage (voir la PR) :
//!
//! - l'éruption d'un drop (`erupt = 1` d'un coup) passe par le limiteur de flashs (règle 7) : le niveau que lit le shader est
//!   celui du limiteur (montée d'au moins `MIN_FADE`, au plus trois départs par seconde), retombée en 1,6 s comme
//!   `ERUPT_DECAY`. Flashs coupés ou respiration : pas d'éruption, la lave coule ;
//! - le défilement du flux est intégré côté CPU (`scroll`) : `time × flow` de Sillage téléportait tout le motif à chaque
//!   variation des basses.

use bevy::math::{Quat, Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::part::{BodySpec, Look, MeshSpec, PartPaint, PartSpec};

const SPAN: f32 = 40.0;
const HEIGHT: f32 = 76.0;
const DEPTH: f32 = 26.0;
/// Flaque de lave au pied de la façade, sur le trottoir.
const POOL_DEPTH: f32 = 9.0;
/// Une éruption retombe en 1,6 s ; elle remonte la façade de bas en haut pendant ce temps.
const ERUPT_DECAY_S: f32 = 1.6;
/// Période de remise à zéro du défilement (garde la précision `f32`).
const SCROLL_WRAP: f64 = 65_536.0;

/// La façade de lave.
#[derive(Debug)]
pub struct LavaBuilding {
    burst: Burst,
    scroll: f64,
    light: Vec3,
}

impl Default for LavaBuilding {
    fn default() -> Self {
        Self::new()
    }
}

impl LavaBuilding {
    /// L'immeuble de lave.
    #[must_use]
    pub fn new() -> Self {
        Self {
            burst: Burst::default(),
            scroll: 0.0,
            light: Vec3::ZERO,
        }
    }

    /// L'éruption en cours, 0..1 (déjà bornée par le limiteur).
    #[must_use]
    pub fn erupt(&self) -> f32 {
        self.burst.level()
    }
}

impl Landmark for LavaBuilding {
    fn kind(&self) -> LandmarkKind {
        LandmarkKind::Lava
    }

    fn span(&self) -> f32 {
        SPAN
    }

    fn body(&self) -> Option<BodySpec> {
        Some(BodySpec {
            height: HEIGHT,
            depth: DEPTH,
            seed: 666,
        })
    }

    fn body_tint(&self) -> Option<SceneColor> {
        Some(SceneColor::Amber)
    }

    fn parts(&self) -> Vec<PartSpec> {
        vec![
            PartSpec::new("façade", MeshSpec::Quad { w: SPAN, h: HEIGHT }, Look::LavaFacade),
            PartSpec::new(
                "flaque",
                MeshSpec::Quad {
                    w: SPAN + 6.0,
                    h: POOL_DEPTH,
                },
                Look::LavaPool,
            ),
        ]
    }

    fn show(&mut self, _place: &Placement) {
        self.burst.reset();
    }

    fn hide(&mut self) {
        self.burst.reset();
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        self.burst.step(frame.dt, frame.policy.permitted());
        if frame.drive.drop {
            // Refusé s'il est interdit ou trop rapproché : la lave coule, sans éruption.
            self.burst.trigger(ERUPT_DECAY_S, frame.time);
        }
        let erupt = self.burst.level();
        let flow = 2.2 + frame.beat.bass * 5.0 + frame.drive.intensity * 3.0;
        self.scroll = (self.scroll + f64::from(flow * frame.dt)) % SCROLL_WRAP;

        // Le feu vire au jaune sur chaque kick et pendant l'éruption.
        let colors = &frame.colors;
        let mix = (0.45 + frame.beat.pulse * 0.4 + erupt * 0.5).min(1.0);
        self.light = colors.get(SceneColor::Alarm).lerp(colors.get(SceneColor::Amber), mix);
        let power = 1.1 + frame.beat.pulse * 1.6 + frame.beat.bass * 1.2 + erupt * 2.0;
        for (i, y) in [10.0, 30.0, 52.0].into_iter().enumerate() {
            ctx.light(i, Vec3::new(0.0, y, 8.0), 110.0, self.light, power);
        }
        // La flaque éclaire le trottoir et la route, qui la reflète.
        ctx.light(3, Vec3::new(0.0, 1.5, 5.0), 75.0, self.light, power * 1.4);
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let [facade, pool] = out else { return };
        let params = |transform: Transform| PartPaint {
            transform,
            mood: self.light,
            a: Vec4::new(self.burst.level(), 0.0, 0.0, 0.0),
            b: Vec4::new(self.scroll as f32, 0.0, 0.0, 0.0),
            ..PartPaint::default()
        };
        *facade = params(Transform::from_xyz(0.0, HEIGHT / 2.0, 0.2));
        *pool = params(
            Transform::from_xyz(0.0, 0.06, POOL_DEPTH / 2.0)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landmarks::field::{FieldState, MAX_LIGHTS};
    use crate::landmarks::frame::FlashPolicy;

    fn run(policy: FlashPolicy, drop_at: usize, frames: usize) -> (Vec<f32>, FieldState) {
        let mut building = LavaBuilding::new();
        let place = Placement {
            z: -1000.0,
            side: 1,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        };
        building.show(&place);
        let mut field = FieldState::default();
        let mut levels = [0.0; MAX_LIGHTS];
        let mut erupts = Vec::new();
        for i in 0..frames {
            let mut frame = LandmarkFrame::rest();
            frame.policy = policy;
            frame.time = i as f64 / 60.0;
            frame.drive.drop = i == drop_at;
            let mut ctx = StepCtx::new(&mut field, &place, &mut levels, frame.dt);
            building.step(&frame, &mut ctx);
            erupts.push(building.erupt());
        }
        (erupts, field)
    }

    const LIVE: FlashPolicy = FlashPolicy {
        flashes: true,
        breathing: false,
    };

    #[test]
    fn un_drop_declenche_une_eruption_qui_monte_et_retombe_sans_a_coup() {
        let (erupts, _) = run(LIVE, 10, 300);
        let peak = erupts.iter().copied().fold(0.0, f32::max);
        assert!(peak > 0.95, "pic {peak}");
        let max_jump = erupts.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(
            max_jump <= (1.0 / 60.0) / cf_show::limiter::MIN_FADE + 1e-3,
            "saut {max_jump}"
        );
        assert!(erupts.last().copied().unwrap_or(1.0) < 0.01, "retombée en 1,6 s");
    }

    #[test]
    fn flashs_coupes_ou_respiration_pas_d_eruption() {
        for policy in [
            FlashPolicy::NONE,
            FlashPolicy {
                flashes: false,
                breathing: false,
            },
            FlashPolicy {
                flashes: true,
                breathing: true,
            },
        ] {
            let (erupts, _) = run(policy, 10, 120);
            assert!(erupts.iter().all(|e| *e == 0.0), "{policy:?}");
        }
    }

    #[test]
    fn quatre_lumieres_orange_dont_une_sur_la_flaque() {
        let (_, field) = run(LIVE, usize::MAX, 90);
        let lit: Vec<_> = field.lights().iter().filter(|l| l.color.length() > 0.0).collect();
        // Les couleurs par défaut du test sont blanches : on vérifie le nombre et la position, pas la teinte.
        assert_eq!(lit.len(), 4);
        assert!((lit[3].radius - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn le_defilement_est_continu_et_ne_depend_pas_de_l_horloge_absolue() {
        let mut building = LavaBuilding::new();
        let place = Placement {
            z: 0.0,
            side: -1,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        };
        building.show(&place);
        let mut field = FieldState::default();
        let mut levels = [0.0; MAX_LIGHTS];
        let mut last = 0.0_f64;
        for i in 0..200 {
            let mut frame = LandmarkFrame::rest();
            frame.time = 5000.0 + f64::from(i) / 60.0;
            frame.beat.bass = if i % 2 == 0 { 1.0 } else { 0.0 };
            let mut ctx = StepCtx::new(&mut field, &place, &mut levels, frame.dt);
            building.step(&frame, &mut ctx);
            assert!(
                building.scroll - last < 0.5,
                "le motif ne saute pas quand les basses varient"
            );
            last = building.scroll;
        }
    }
}
