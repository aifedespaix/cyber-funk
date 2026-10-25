//! L'hologramme arcade (`arcade-building.ts`) : un bâtiment-projecteur dont la lentille dessine, flottant devant la façade,
//! une partie de Space Invaders qui joue toute seule.
//!
//! La partie est une simulation CPU de 72 × 48 cases ([`ArcadeGame`](super::arcade_game::ArcadeGame)), écrite dans une
//! texture à un octet par case ; le shader (`landmark_arcade.wesl`) convertit chaque indice en couleur de palette. Les sprites
//! bougent sur les kicks, tirent sur les aigus ; un drop invoque un boss. Un faisceau relie la lentille à l'image, et deux
//! lumières colorent la rue selon l'action.
//!
//! Écarts avec Sillage (règle 7 de `CLAUDE.md`) : la déchirure de l'image après un drop (`glitch = 1`, retombée en 1,4 s)
//! passe par un limiteur de flashs ; l'opacité du faisceau, qui suivait l'énergie de la partie d'image en image, en suit
//! une version à pente bornée (`slew`).

use bevy::math::{Vec2, Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::arcade_game::{ArcadeGame, GRID_H, GRID_W};
use super::flash::{Burst, slew};
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::part::{BodySpec, Look, MapId, MeshData, MeshSpec, PartPaint, PartSpec};

const SPAN: f32 = 54.0;
const HEIGHT: f32 = 44.0;
const DEPTH: f32 = 24.0;
const HOLO_W: f32 = 54.0;
const HOLO_H: f32 = HOLO_W * (GRID_H as f32 / GRID_W as f32);
const HOLO_Y: f32 = 28.0 + HOLO_H / 2.0;
/// L'hologramme flotte à cette distance de la façade, au-dessus du trottoir et du bord de route.
const HOLO_Z: f32 = 10.0;
const LENS_Y: f32 = 14.0;
/// Plans de l'hologramme : avant (net), milieu, arrière (froid et pâle). Le volume se lit en parallaxe.
const LAYER_Z: [f32; 3] = [1.1, 0.0, -1.3];
/// Retombée de la déchirure d'un drop : `1 / 0,7` de Sillage, en s.
const GLITCH_DECAY_S: f32 = 1.43;

/// L'hologramme arcade.
#[derive(Debug)]
pub struct ArcadeBuilding {
    game: ArcadeGame,
    burst: Burst,
    light: Vec3,
    beam_amount: f32,
    glitch: f32,
}

impl Default for ArcadeBuilding {
    fn default() -> Self {
        Self::new()
    }
}

impl ArcadeBuilding {
    /// L'immeuble arcade, sa partie neuve.
    #[must_use]
    pub fn new() -> Self {
        Self {
            game: ArcadeGame::new(0x0A2C_ADE7),
            burst: Burst::default(),
            light: Vec3::ZERO,
            beam_amount: 0.0,
            glitch: 0.0,
        }
    }

    /// La partie en cours.
    #[must_use]
    pub fn game(&self) -> &ArcadeGame {
        &self.game
    }

    /// La déchirure de l'image, 0..1 (celle du limiteur).
    #[must_use]
    pub fn glitch(&self) -> f32 {
        self.glitch
    }

    /// Les trois plans de l'hologramme, la couche de chaque sommet dans `uv1.x`.
    fn holo_mesh() -> MeshData {
        let mut mesh = MeshData::default();
        for z in LAYER_Z {
            mesh.push_quad(Vec3::new(0.0, 0.0, z), Vec2::new(HOLO_W, HOLO_H));
            let layer = if z > 0.0 {
                1.0
            } else if z == 0.0 {
                0.4
            } else {
                0.0
            };
            mesh.uv1.extend([[layer, 0.0]; 4]);
        }
        mesh
    }
}

impl Landmark for ArcadeBuilding {
    fn kind(&self) -> LandmarkKind {
        LandmarkKind::Arcade
    }

    fn span(&self) -> f32 {
        SPAN
    }

    fn body(&self) -> Option<BodySpec> {
        Some(BodySpec {
            height: HEIGHT,
            depth: DEPTH,
            seed: 77,
        })
    }

    fn body_tint(&self) -> Option<SceneColor> {
        Some(SceneColor::Violet)
    }

    fn parts(&self) -> Vec<PartSpec> {
        vec![
            PartSpec::new(
                "hologramme",
                MeshSpec::Custom(Box::new(Self::holo_mesh())),
                Look::ArcadeHolo,
            )
            .with_map(MapId::ArcadeGrid),
            PartSpec::new("faisceau", MeshSpec::Custom(Box::new(MeshData::pyramid())), Look::Beam),
        ]
    }

    fn show(&mut self, _place: &Placement) {
        self.burst.reset();
        self.glitch = 0.0;
        self.beam_amount = 0.0;
    }

    fn hide(&mut self) {
        self.burst.reset();
        self.beam_amount = 0.0;
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        self.burst.step(frame.dt, frame.policy.permitted());
        if frame.beat.kick {
            self.game.on_beat();
        }
        if frame.drive.drop {
            self.game.start_boss();
            self.burst.trigger(GLITCH_DECAY_S, frame.time);
        }
        self.game.advance(frame.dt, frame.beat.beat, frame.beat.high);
        self.glitch = self.burst.level();

        // La lentille projette vers l'image ; le faisceau s'intensifie avec l'action.
        self.beam_amount = slew(self.beam_amount, 0.55 + self.game.energy * 0.8, frame.dt);
        // Deux lumières devant l'image : cyan en temps normal, rouge pendant le boss, jaune aux explosions.
        let colors = &frame.colors;
        let base = if self.game.boss_active() {
            colors.get(SceneColor::Alarm)
        } else {
            colors.get(SceneColor::Mint)
        };
        self.light = base.lerp(colors.get(SceneColor::Sun), self.game.energy * 0.6);
        let power = 0.9 + frame.beat.pulse * 1.1 + self.game.energy * 1.6;
        for (i, x) in [-HOLO_W * 0.25, HOLO_W * 0.25].into_iter().enumerate() {
            ctx.light(i, Vec3::new(x, HOLO_Y, HOLO_Z + 3.0), 125.0, self.light, power);
        }
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let [holo, beam] = out else { return };
        *holo = PartPaint {
            transform: Transform::from_xyz(0.0, HOLO_Y, HOLO_Z),
            mood: self.light,
            a: Vec4::new(self.glitch, 0.0, 0.0, 0.0),
            ..PartPaint::default()
        };
        let from = Vec3::new(0.0, LENS_Y, 1.0);
        let to = Vec3::new(0.0, HOLO_Y, HOLO_Z);
        // La pyramide pointe vers +Z : le repère de Bevy regarde vers −Z, on vise le point symétrique.
        *beam = PartPaint {
            transform: Transform::from_translation(from)
                .looking_at(from - (to - from), Vec3::Y)
                .with_scale(Vec3::new(HOLO_W / 2.0, HOLO_H / 2.0, from.distance(to))),
            mood: Vec3::new(self.light.x, self.light.y, self.light.z),
            a: Vec4::new(self.beam_amount, 0.0, 0.0, 0.0),
            ..PartPaint::default()
        };
    }

    fn grid(&self) -> Option<&[u8]> {
        Some(&self.game.cells)
    }

    fn take_grid_dirty(&mut self) -> bool {
        std::mem::take(&mut self.game.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landmarks::field::{FieldState, MAX_LIGHTS};
    use crate::landmarks::frame::FlashPolicy;

    fn run(policy: FlashPolicy, frames: usize, drop_at: usize) -> (ArcadeBuilding, Vec<f32>) {
        let mut building = ArcadeBuilding::new();
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
        let mut glitches = Vec::new();
        for i in 0..frames {
            let mut frame = LandmarkFrame::rest();
            frame.policy = policy;
            frame.time = i as f64 / 60.0;
            frame.beat.kick = i % 28 == 0;
            frame.beat.beat = i as f32 / 28.0;
            frame.drive.drop = i == drop_at;
            let mut ctx = StepCtx::new(&mut field, &place, &mut levels, frame.dt);
            building.step(&frame, &mut ctx);
            glitches.push(building.glitch());
        }
        (building, glitches)
    }

    #[test]
    fn la_grille_est_envoyee_au_gpu_quand_elle_change() {
        let (mut building, _) = run(FlashPolicy::NONE, 30, usize::MAX);
        assert!(building.take_grid_dirty());
        assert!(!building.take_grid_dirty(), "marquée envoyée");
        assert_eq!(building.grid().map(<[u8]>::len), Some(GRID_W * GRID_H));
    }

    #[test]
    fn le_drop_appelle_le_boss_et_la_dechirure_est_bornee() {
        let (building, glitches) = run(
            FlashPolicy {
                flashes: true,
                breathing: false,
            },
            120,
            20,
        );
        assert!(building.game().boss_active());
        let peak = glitches.iter().copied().fold(0.0, f32::max);
        assert!(peak > 0.9);
        let max_jump = glitches.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_jump <= (1.0 / 60.0) / cf_show::limiter::MIN_FADE + 1e-3);
    }

    #[test]
    fn flashs_coupes_pas_de_dechirure_mais_le_boss_vient_quand_meme() {
        let (building, glitches) = run(FlashPolicy::NONE, 120, 20);
        assert!(glitches.iter().all(|g| *g == 0.0));
        assert!(building.game().boss_active());
    }

    #[test]
    fn l_opacite_du_faisceau_suit_l_energie_a_pente_bornee() {
        let (building, _) = run(FlashPolicy::NONE, 5, 1);
        // Le boss met l'énergie à 1 d'un coup ; le faisceau, parti de 0, n'a pas pu monter plus vite que la borne.
        assert!(building.beam_amount <= 5.0 * (1.0 / 60.0) / cf_show::limiter::MIN_FADE + 1e-3);
    }

    #[test]
    fn deux_pieces_dont_l_hologramme_a_trois_plans_et_la_texture_de_la_grille() {
        let parts = ArcadeBuilding::new().parts();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].map, Some(MapId::ArcadeGrid));
        let MeshSpec::Custom(mesh) = &parts[0].mesh else {
            panic!("maillage fusionné");
        };
        assert_eq!(mesh.vertex_count(), 12);
        assert_eq!(mesh.uv1.iter().filter(|l| l[0] > 0.9).count(), 4);
    }
}
