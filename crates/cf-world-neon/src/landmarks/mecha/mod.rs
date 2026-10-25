//! Le chœur de mecha-immeubles (`mecha-building.ts`) : six robots géants, trois de chaque côté de la route, décalés pour ne
//! jamais se faire face ni se toucher. Chacun a sa couleur, sa taille, son décalage dans la chorégraphie et son tic (bras
//! inversés, danse plus lente ou plus nerveuse). Leurs jambes plongent à des centaines de mètres sous la route, ce qui les
//! rend colossaux.
//!
//! L'immeuble enjambe la route (repère monde, sans rotation) ; chaque robot a sa lumière dynamique (six au total, le champ de
//! la ville n'en offre pas plus).
//!
//! - [`dance`] : les poses et la chorégraphie ;
//! - [`robot`] : un robot, de sa cinématique à son visage.

pub mod dance;
pub mod robot;

use cf_render::SceneColor;

use self::robot::{PARTS, REACH, Robot, RobotSpec};
use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{FACE_X, Landmark, LandmarkKind, Placement, StepCtx};
use super::part::{PartPaint, PartSpec, RoadBand};

/// Chaque robot se tient en retrait de la ligne des immeubles : tronc centré à 12 m derrière la façade.
const STANCE_X: f32 = FACE_X + 12.0;
/// Écart le long de la route entre deux robots du même côté : plus que la portée du plus grand, bras écartés.
const GAP: f32 = 124.0;

/// Une place du chœur.
struct Layout {
    side: i8,
    /// Position le long de la route, en écarts. Les deux côtés sont décalés d'un demi-écart : aucun robot n'est face à un
    /// autre.
    at: f32,
    scale: f32,
    accent: SceneColor,
    beat_offset: u32,
    level_shift: i8,
    mirror: bool,
    every: u32,
    twist_gain: f32,
    look_speed: f32,
}

#[rustfmt::skip]
const fn layout(side: i8, at: f32, scale: f32, accent: SceneColor, beat_offset: u32, level_shift: i8, mirror: bool, every: u32, twist_gain: f32, look_speed: f32) -> Layout {
    Layout { side, at, scale, accent, beat_offset, level_shift, mirror, every, twist_gain, look_speed }
}

/// Trois robots de chaque côté, chacun avec sa couleur, sa taille et sa façon de bouger.
const ROSTER: [Layout; 6] = [
    layout(1, -1.25, 1.0, SceneColor::Sun, 0, 0, false, 1, 1.0, 1.0),
    layout(-1, -0.75, 1.15, SceneColor::Mint, 3, 0, true, 1, 1.4, 0.6),
    layout(1, -0.25, 0.9, SceneColor::Rose, 5, 1, true, 1, 0.7, 1.6),
    layout(-1, 0.25, 1.2, SceneColor::Azure, 2, 0, false, 2, 1.2, 0.5),
    layout(1, 0.75, 1.05, SceneColor::Violet, 6, -1, false, 1, 1.0, 1.2),
    layout(-1, 1.25, 0.95, SceneColor::Lime, 1, 1, true, 1, 0.8, 2.0),
];

/// Le chœur de mecha.
#[derive(Debug)]
pub struct MechaBuilding {
    robots: Vec<Robot>,
    /// Le limiteur partagé des déchirures de visage (règle 7 de `CLAUDE.md`).
    tear: Burst,
    cache: Vec<PartPaint>,
    edge: f32,
}

impl Default for MechaBuilding {
    fn default() -> Self {
        Self::new()
    }
}

impl MechaBuilding {
    /// Le chœur, au repos.
    #[must_use]
    pub fn new() -> Self {
        let edge = ROSTER.iter().map(|r| r.at.abs()).fold(0.0, f32::max) * GAP + REACH * 1.2;
        let robots = ROSTER
            .iter()
            .enumerate()
            .map(|(slot, l)| {
                Robot::new(RobotSpec {
                    side: l.side,
                    x: STANCE_X,
                    z: l.at * GAP,
                    scale: l.scale,
                    accent: l.accent,
                    beat_offset: l.beat_offset,
                    level_shift: l.level_shift,
                    mirror: l.mirror,
                    every: l.every,
                    twist_gain: l.twist_gain,
                    look_speed: l.look_speed,
                    slot,
                    seed: 9 + slot as u32 * 40,
                })
            })
            .collect();
        Self {
            robots,
            tear: Burst::default(),
            cache: vec![PartPaint::default(); ROSTER.len() * PARTS],
            edge,
        }
    }

    /// Les robots (lecture, tests).
    #[must_use]
    pub fn robots(&self) -> &[Robot] {
        &self.robots
    }
}

impl Landmark for MechaBuilding {
    fn kind(&self) -> LandmarkKind {
        LandmarkKind::Mecha
    }

    fn span(&self) -> f32 {
        self.edge * 2.0
    }

    fn spans_road(&self) -> bool {
        true
    }

    /// Les bras écartés des robots passent au-dessus de la route, à partir de la taille : sous elle, la route est libre.
    fn road_band(&self) -> Option<RoadBand> {
        Some(RoadBand {
            lo: 36.0,
            hi: 110.0,
            half_depth: self.edge,
        })
    }

    fn parts(&self) -> Vec<PartSpec> {
        (0..ROSTER.len()).flat_map(|_| robot::parts()).collect()
    }

    fn show(&mut self, _place: &Placement) {
        self.tear.reset();
        for robot in &mut self.robots {
            robot.reset();
        }
    }

    fn hide(&mut self) {
        self.tear.reset();
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        let permitted = frame.policy.permitted();
        let Self {
            robots, tear, cache, ..
        } = self;
        tear.step(frame.dt, permitted);
        for (index, robot) in robots.iter_mut().enumerate() {
            if frame.beat.kick {
                robot.kick(frame);
            }
            if frame.drive.drop {
                robot.drop_hit();
            }
            let level = tear.level();
            robot.step(
                frame,
                permitted,
                &mut || tear.trigger_scaled(1.0, robot::TEAR_DECAY_S, frame.time),
                level,
            );
            robot.paint(&mut cache[index * PARTS..(index + 1) * PARTS]);
            // Une seule lumière réelle par robot (le champ n'en offre que six) : à hauteur de la poitrine, large.
            let (position, radius, color, power) = robot.light(frame);
            ctx.light(index, position, radius, color, power);
        }
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let n = out.len().min(self.cache.len());
        out[..n].copy_from_slice(&self.cache[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landmarks::field::{FieldState, MAX_LIGHTS};
    use crate::landmarks::frame::FlashPolicy;

    #[test]
    fn le_choeur_a_six_robots_decales_qui_ne_se_font_jamais_face() {
        let mecha = MechaBuilding::new();
        assert_eq!(mecha.parts().len(), 6 * PARTS);
        for side in [-1_i8, 1] {
            let mut zs: Vec<f32> = ROSTER.iter().filter(|r| r.side == side).map(|r| r.at * GAP).collect();
            zs.sort_by(f32::total_cmp);
            assert_eq!(zs.len(), 3);
            // Plus que la portée du plus grand, bras écartés : deux voisins du même côté ne se touchent pas.
            assert!(zs.windows(2).all(|w| w[1] - w[0] >= GAP - 1e-3));
        }
        for a in ROSTER.iter().filter(|r| r.side == 1) {
            for b in ROSTER.iter().filter(|r| r.side == -1) {
                assert!(((a.at - b.at) * GAP).abs() >= GAP / 2.0 - 1e-3, "face à face");
            }
        }
    }

    #[test]
    fn l_emprise_est_celle_de_sillage() {
        let mecha = MechaBuilding::new();
        let edge = 1.25 * 124.0 + REACH * 1.2;
        assert!((mecha.span() - edge * 2.0).abs() < 1e-3);
        let band = mecha.road_band().expect("bande");
        assert!((band.lo - 36.0).abs() < f32::EPSILON && (band.hi - 110.0).abs() < f32::EPSILON);
        assert!(mecha.spans_road() && mecha.body().is_none());
    }

    fn run(policy: FlashPolicy, frames: usize) -> (MechaBuilding, FieldState) {
        let mut mecha = MechaBuilding::new();
        let place = Placement {
            z: -5000.0,
            side: 1,
            tint: 0,
            scale: 1.0,
            spans_road: true,
        };
        mecha.show(&place);
        let mut field = FieldState::default();
        let mut levels = [0.0; MAX_LIGHTS];
        for i in 0..frames {
            let mut frame = LandmarkFrame::rest();
            frame.policy = policy;
            frame.time = i as f64 / 60.0;
            frame.beat.kick = i % 28 == 0;
            frame.beat.count = (i / 28) as u32;
            frame.drive.drop = i == 200;
            frame.drive.section = cf_analysis::Section::Peak;
            let mut ctx = StepCtx::new(&mut field, &place, &mut levels, frame.dt);
            mecha.step(&frame, &mut ctx);
        }
        (mecha, field)
    }

    #[test]
    fn six_lumieres_une_par_robot() {
        let (_, field) = run(FlashPolicy::NONE, 120);
        assert_eq!(field.lights().iter().filter(|l| l.color.length() > 0.0).count(), 6);
    }

    #[test]
    fn au_repos_aucun_robot_ne_se_dechire_ni_ne_devient_agressif() {
        let (mecha, _) = run(FlashPolicy::NONE, 600);
        for robot in mecha.robots() {
            assert!(!robot.face().aggressive());
            assert!(robot.tear().abs() < f32::EPSILON);
        }
    }

    #[test]
    fn permis_au_sommet_les_visages_se_durcissent_et_la_dechirure_reste_bornee() {
        let (mecha, _) = run(
            FlashPolicy {
                flashes: true,
                breathing: false,
            },
            400,
        );
        assert!(mecha.robots().iter().any(|r| r.face().aggressive()));
        let mut paint = vec![PartPaint::default(); 6 * PARTS];
        mecha.paint(&LandmarkFrame::rest(), &mut paint);
        // La déchirure est le niveau d'un limiteur : jamais au-delà de 1.
        for robot in 0..6 {
            let screen = &paint[robot * PARTS + 5];
            assert!((0.0..=1.0 + 1e-6).contains(&screen.a.y));
        }
    }

    #[test]
    fn toutes_les_poses_sont_finies_et_les_pieds_sous_la_route() {
        let (mecha, _) = run(
            FlashPolicy {
                flashes: true,
                breathing: false,
            },
            300,
        );
        let mut paint = vec![PartPaint::default(); 6 * PARTS];
        mecha.paint(&LandmarkFrame::rest(), &mut paint);
        for part in &paint {
            assert!(
                part.transform.translation.is_finite()
                    && part.transform.scale.is_finite()
                    && part.transform.rotation.is_finite()
            );
        }
        let lowest = paint
            .iter()
            .map(|p| p.transform.translation.y - p.transform.scale.y / 2.0)
            .fold(f32::INFINITY, f32::min);
        assert!(lowest < -380.0, "{lowest}");
    }
}
