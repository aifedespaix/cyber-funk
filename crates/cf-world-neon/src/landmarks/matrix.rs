//! La façade « Matrix Data » (`matrix-building.ts`) : une pluie de glyphes sur quatre plans en profondeur, en trois
//! variantes (`rain`, `flux`, `binary`).
//!
//! Approche de Sillage, gardée : pas de texture. Chaque plan est un calcul de colonnes dans le fragment
//! (`landmark_matrix_wall.wesl`), décalé par parallaxe selon la position de la caméra dans le repère de la façade. Devant
//! le mur, un petit essaim de grands glyphes tombe à quelques mètres : ceux-là sont de vraie géométrie (un maillage fusionné,
//! `landmark_matrix_sprouts.wesl`). Au drop, la couleur du flux bascule (rouge, cyan, arc-en-ciel).
//!
//! Écart avec Sillage (règle 7 de `CLAUDE.md`) : la bascule de palette était instantanée sur tout le mur. Elle passe par un
//! limiteur (un départ à la fois, au plus trois par seconde) et se **fond** en plus de `MIN_FADE` ; flashs coupés ou mode
//! respiration : le flux reste vert terminal, l'état de repos. Le défilement est intégré côté CPU (`speed × time` de
//! Sillage téléportait les rangées à chaque variation des basses). L'arc-en-ciel des lumières glisse sur le cycle au lieu de
//! changer de teinte quatre fois par seconde.

use bevy::math::{Quat, Vec2, Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::math::damp;
use super::part::{BodySpec, Look, MeshData, MeshSpec, PartPaint, PartSpec};
use crate::pilot::rand::Rand;

const SPAN: f32 = 36.0;
const HEIGHT: f32 = 74.0;
const DEPTH: f32 = 26.0;
/// Caractères qui jaillissent du mur : de gros glyphes nets, à plusieurs mètres de la façade.
const SPROUTS: usize = 70;
const SPROUT_OUT: f32 = 7.0;
/// Après un drop, la palette bascule pendant ce nombre de temps avant de revenir au vert.
const MODE_BEATS: u32 = 16;
/// Palettes du flux : 0 vert terminal, 1 rouge sang, 2 cyan électrique, 3 arc-en-ciel qui balaie.
const MODES: u32 = 4;
/// Vitesse de fondu de la couleur du flux (1/s) : moins de 1 / `MIN_FADE`, la pente d'un fondu ne dépasse jamais la borne.
const FADE_RATE: f32 = 8.0;
/// Durée de retombée demandée au limiteur pour une bascule (s).
const SWITCH_DECAY_S: f32 = 0.5;

/// Les trois façons de faire couler le code : pluie verticale, courants horizontaux, chiffres binaires avec balayage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixVariant {
    /// Pluie verticale.
    Rain,
    /// Courants horizontaux.
    Flux,
    /// Chiffres binaires, balayage et glitchs.
    Binary,
}

impl MatrixVariant {
    const fn seed(self) -> u32 {
        match self {
            Self::Rain => 131,
            Self::Flux => 137,
            Self::Binary => 149,
        }
    }

    const fn kind(self) -> LandmarkKind {
        match self {
            Self::Rain => LandmarkKind::Matrix,
            Self::Flux => LandmarkKind::Flux,
            Self::Binary => LandmarkKind::Binary,
        }
    }

    const fn index(self) -> f32 {
        match self {
            Self::Rain => 0.0,
            Self::Flux => 1.0,
            Self::Binary => 2.0,
        }
    }
}

/// Le mur de code.
#[derive(Debug)]
pub struct MatrixBuilding {
    variant: MatrixVariant,
    rng: Rand,
    burst: Burst,
    side: i8,
    mode: u32,
    mode_beats: u32,
    last_mode: u32,
    /// Teinte courante du flux, fondue vers `tint_target`.
    tint: Vec3,
    tint_target: Vec3,
    /// Part d'arc-en-ciel, fondue vers `rainbow_target`.
    rainbow: f32,
    rainbow_target: f32,
    light: Vec3,
    colors_ready: bool,
    /// Défilement du code, intégré côté CPU (voir l'écart de la PR).
    scroll: f64,
}

impl MatrixBuilding {
    /// Le mur de code de cette variante.
    #[must_use]
    pub fn new(variant: MatrixVariant) -> Self {
        Self {
            variant,
            rng: Rand::new(u64::from(variant.seed()) ^ 0x4D41_5452),
            burst: Burst::default(),
            side: 1,
            mode: 0,
            mode_beats: 0,
            last_mode: 0,
            tint: Vec3::ZERO,
            tint_target: Vec3::ZERO,
            rainbow: 0.0,
            rainbow_target: 0.0,
            light: Vec3::ZERO,
            colors_ready: false,
            scroll: 0.0,
        }
    }

    /// La palette courante du flux (0 : vert de repos).
    #[must_use]
    pub fn mode(&self) -> u32 {
        self.mode
    }

    fn set_mode(&mut self, mode: u32, frame: &LandmarkFrame) {
        self.mode = mode;
        self.mode_beats = 0;
        let colors = &frame.colors;
        self.tint_target = match mode.min(2) {
            0 => colors.get(SceneColor::Lime),
            1 => colors.get(SceneColor::Alarm),
            _ => colors.get(SceneColor::Mint),
        };
        self.rainbow_target = if mode > 2 { 1.0 } else { 0.0 };
    }

    /// Les glyphes qui jaillissent : un plan par glyphe, `(x, phase, profondeur, vitesse)` dans la couleur des sommets.
    fn sprouts_mesh(&self) -> MeshData {
        let mut rng = Rand::new(u64::from(self.variant.seed()) ^ 0x5352_4F55);
        let mut mesh = MeshData::default();
        for _ in 0..SPROUTS {
            let seed = [
                rng.unit() as f32,
                rng.unit() as f32,
                1.0 + rng.unit() as f32 * SPROUT_OUT,
                0.6 + rng.unit() as f32 * 1.6,
            ];
            let first = mesh.push_quad(Vec3::ZERO, Vec2::ONE);
            debug_assert_eq!(first as usize % 4, 0);
            mesh.colors.extend([seed; 4]);
        }
        mesh
    }
}

impl Landmark for MatrixBuilding {
    fn kind(&self) -> LandmarkKind {
        self.variant.kind()
    }

    fn span(&self) -> f32 {
        SPAN
    }

    fn body(&self) -> Option<BodySpec> {
        Some(BodySpec {
            height: HEIGHT,
            depth: DEPTH,
            seed: self.variant.seed(),
        })
    }

    fn body_tint(&self) -> Option<SceneColor> {
        Some(SceneColor::Lime)
    }

    fn parts(&self) -> Vec<PartSpec> {
        vec![
            PartSpec::new("mur", MeshSpec::Quad { w: SPAN, h: HEIGHT }, Look::MatrixWall),
            PartSpec::new("bout du mur", MeshSpec::Quad { w: DEPTH, h: HEIGHT }, Look::MatrixWall),
            // Les glyphes jaillissent jusqu'à `SPROUT_OUT` + la taille d'un glyphe devant la façade, sur toute la largeur et la
            // hauteur : l'enveloppe est celle de ce volume (le vertex shader déplace les sommets).
            PartSpec::new(
                "glyphes",
                MeshSpec::Custom(Box::new(self.sprouts_mesh())),
                Look::MatrixSprouts,
            )
            .with_bounds(
                Vec3::new(-SPAN * 0.5 - 4.0, -4.0, -2.0),
                Vec3::new(SPAN * 0.5 + 4.0, HEIGHT + 4.0, SPROUT_OUT + 5.0),
            ),
        ]
    }

    fn show(&mut self, place: &Placement) {
        self.side = place.side;
        self.mode = 0;
        self.mode_beats = 0;
        self.rainbow = 0.0;
        self.rainbow_target = 0.0;
        self.colors_ready = false;
        self.burst.reset();
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        let colors = &frame.colors;
        if !self.colors_ready {
            // Première image du passage : le vert, sans fondu (l'immeuble naît dans cette couleur).
            self.set_mode(0, frame);
            self.tint = self.tint_target;
            self.colors_ready = true;
        }
        self.burst.step(frame.dt, frame.policy.permitted());
        // Le défilement : `speed × time` de Sillage téléportait les rangées à chaque variation des basses.
        let rate = 1.0 + frame.beat.bass * 0.9 + frame.drive.intensity * 0.6;
        self.scroll = (self.scroll + f64::from(rate * frame.dt)) % 65_536.0;
        if frame.beat.kick && self.mode != 0 {
            self.mode_beats += 1;
            if self.mode_beats >= MODE_BEATS {
                self.set_mode(0, frame);
            }
        }
        if frame.drive.drop && self.burst.trigger(SWITCH_DECAY_S, frame.time) {
            // Jamais deux fois la même palette de suite, et jamais de retour au vert avant la fin du délai.
            let mut next = 1 + (self.rng.unit() * f64::from(MODES - 1)) as u32;
            if next == self.last_mode {
                next = 1 + (next % (MODES - 1));
            }
            self.last_mode = next;
            self.set_mode(next, frame);
        }
        // Le fondu : la couleur glisse (au moins `MIN_FADE` d'un bout à l'autre, voir `FADE_RATE`).
        for axis in 0..3 {
            self.tint[axis] = damp(self.tint[axis], self.tint_target[axis], FADE_RATE, frame.dt);
        }
        self.rainbow = damp(self.rainbow, self.rainbow_target, FADE_RATE, frame.dt);

        // Les lumières prennent la couleur courante du flux (l'arc-en-ciel défile sur le cycle, sans saut).
        let cycle = colors.cycle();
        let t = (frame.time * 4.0).max(0.0);
        let i = t.floor() as usize;
        let sweep = cycle[i % cycle.len()].lerp(cycle[(i + 1) % cycle.len()], t.fract() as f32);
        self.light = self.tint.lerp(sweep, self.rainbow);
        let power = 0.9 + frame.beat.pulse * 1.8 + frame.beat.bass;
        ctx.light(0, Vec3::new(0.0, HEIGHT * 0.3, 14.0), 120.0, self.light, power);
        ctx.light(1, Vec3::new(0.0, HEIGHT * 0.75, 14.0), 120.0, self.light, power);
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let a = self.tint.extend(self.rainbow);
        let paint = |width: f32, transform: Transform| PartPaint {
            transform,
            mood: self.light,
            a,
            b: Vec4::new(width, self.variant.index(), 0.0, 0.0),
            c: Vec4::new(self.scroll as f32, 0.0, 0.0, 0.0),
            ..PartPaint::default()
        };
        let side = f32::from(self.side);
        let [wall, front, sprouts] = out else { return };
        *wall = paint(SPAN, Transform::from_xyz(0.0, HEIGHT / 2.0, 0.2));
        // Façade d'entrée tournée vers le conducteur qui arrive : le mur se lit des deux côtés. Le bout du bloc qui regarde
        // vers la voiture (z monde plus grand) est à `+side × demi-largeur`.
        *front = paint(
            DEPTH,
            Transform::from_xyz(side * (SPAN / 2.0 + 0.2), HEIGHT / 2.0, -DEPTH / 2.0)
                .with_rotation(Quat::from_rotation_y(side * std::f32::consts::FRAC_PI_2)),
        );
        *sprouts = paint(SPAN, Transform::IDENTITY);
    }
}

#[cfg(test)]
mod tests;
