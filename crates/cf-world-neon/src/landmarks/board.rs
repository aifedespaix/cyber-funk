//! La tour « Carte mère » (`board-tower.ts`) : la façade est une carte vue de dessus, procédurale dans un fragment
//! (`landmark_board.wesl`). Deux lumières mint ; la vitesse des signaux suit l'intensité de la piste, un coup de grosse
//! caisse les rallume (par le pouls borné du limiteur, règle 7).
//!
//! Écart avec Sillage : le défilement des signaux est intégré côté CPU (`time × speed` téléportait les signaux à chaque
//! variation des basses).

use bevy::math::{Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::part::{BodySpec, Look, MeshSpec, PartPaint, PartSpec};

const SPAN: f32 = 36.0;
const HEIGHT: f32 = 84.0;
const DEPTH: f32 = 24.0;
const SCROLL_WRAP: f64 = 65_536.0;

/// La tour carte mère.
#[derive(Debug, Default)]
pub struct BoardTower {
    scroll: f64,
    mood: Vec3,
}

impl BoardTower {
    /// La tour.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Landmark for BoardTower {
    fn kind(&self) -> LandmarkKind {
        LandmarkKind::Board
    }

    fn span(&self) -> f32 {
        SPAN
    }

    fn body(&self) -> Option<BodySpec> {
        Some(BodySpec {
            height: HEIGHT,
            depth: DEPTH,
            seed: 808,
        })
    }

    fn body_tint(&self) -> Option<SceneColor> {
        Some(SceneColor::Azure)
    }

    fn parts(&self) -> Vec<PartSpec> {
        vec![PartSpec::new(
            "façade",
            MeshSpec::Quad { w: SPAN, h: HEIGHT },
            Look::BoardFacade,
        )]
    }

    fn show(&mut self, _place: &Placement) {}

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        let speed = 0.25 + frame.drive.intensity * 1.2 + frame.beat.bass * 0.5;
        self.scroll = (self.scroll + f64::from(speed * frame.dt)) % SCROLL_WRAP;
        let mint = frame.colors.get(SceneColor::Mint);
        let power = 0.8 + frame.drive.intensity * 1.2 + frame.beat.pulse * 1.2;
        for (i, y) in [HEIGHT * 0.28, HEIGHT * 0.62].into_iter().enumerate() {
            ctx.light(i, Vec3::new(0.0, y, 12.0), 95.0, mint, power);
        }
        self.mood = mint;
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        if let [facade] = out {
            *facade = PartPaint {
                transform: Transform::from_xyz(0.0, HEIGHT / 2.0, 0.2),
                mood: self.mood,
                b: Vec4::new(self.scroll as f32, 0.0, 0.0, 0.0),
                ..PartPaint::default()
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landmarks::field::{FieldState, MAX_LIGHTS};

    #[test]
    fn deux_lumieres_mint_a_28_et_62_pour_cent_de_la_hauteur() {
        let mut tower = BoardTower::new();
        let place = Placement {
            z: 0.0,
            side: 1,
            tint: 0,
            scale: 1.0,
            spans_road: false,
        };
        tower.show(&place);
        let mut field = FieldState::default();
        let mut levels = [0.0; MAX_LIGHTS];
        for _ in 0..60 {
            let frame = LandmarkFrame::rest();
            let mut ctx = StepCtx::new(&mut field, &place, &mut levels, frame.dt);
            tower.step(&frame, &mut ctx);
        }
        let ys: Vec<f64> = field
            .lights()
            .iter()
            .filter(|l| l.color.length() > 0.0)
            .map(|l| l.position.y)
            .collect();
        assert_eq!(ys.len(), 2);
        assert!((ys[0] - f64::from(HEIGHT * 0.28)).abs() < 1e-3 && (ys[1] - f64::from(HEIGHT * 0.62)).abs() < 1e-3);
        assert_eq!(tower.body().map(|b| (b.seed, b.height)), Some((808, HEIGHT)));
    }
}
