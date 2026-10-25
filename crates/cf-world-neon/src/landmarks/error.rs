//! L'immeuble « ERROR » (`error-building.ts`) : un corps rouge, un panneau géant façon Hollywood sur le toit, et un mot
//! lumineux projeté sur la route qui suit la voiture.
//!
//! Le panneau est un maillage de sept plans qui partagent un même fragment (atlas de texte, tranches décalées, aberration
//! chromatique, lettres qui s'éteignent) : l'extrusion naît de la parallaxe. Le mot au sol n'est pas un objet mais un
//! décalque lu par le shader de la route (même atlas, même glitch, voir [`super::field`]) ; le faisceau qui le relie au
//! panneau est un volume additif. La zone de glitch déborde sur les façades voisines et sur la rue par le champ partagé.
//!
//! Écarts avec Sillage (règle 7 de `CLAUDE.md`) : toute la déchirure (`glitch`) est le niveau d'**un limiteur de flashs**
//! (au plus trois départs par seconde, montée et descente d'au moins `MIN_FADE`) ; le stroboscope à 17 et 18 Hz du cadre du
//! panneau et du décalque est supprimé ; le panneau tremble en sinusoïdes plutôt qu'au hasard à chaque image ; le plancher
//! `0,06 + basses × 0,25` n'existe plus (au repos : flashs coupés ou respiration, il n'y a pas de déchirure du tout).

use bevy::math::{DVec3, Vec2, Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::error_atlas::{ERROR_WORDS, spans};
use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::math::{clamp01, smoothstep};
use super::part::{BodySpec, Look, MapId, MeshData, MeshSpec, PartPaint, PartSpec};
use crate::pilot::rand::Rand;

const SPAN: f32 = 42.0;
const HEIGHT: f32 = 58.0;
const DEPTH: f32 = 30.0;
const SIGN_W: f32 = 60.0;
const SIGN_H: f32 = SIGN_W / 4.0;
const SIGN_Y: f32 = HEIGHT + 10.0 + SIGN_H / 2.0;
/// Couches du panneau : empilées, elles font une lettre extrudée dont la profondeur se lit en parallaxe.
const LAYERS: usize = 7;
const LAYER_GAP: f32 = 0.7;
/// Mot projeté au sol : mêmes proportions 4:1 que l'atlas.
const DECAL_HALF_W: f32 = 7.8;
const DECAL_HALF_D: f32 = DECAL_HALF_W / 4.0;
const DECAL_LEAD: f64 = 15.0;
const WORD_EVERY_KICKS: u32 = 8;
/// Durée pendant laquelle un drop garde le panneau en panne totale.
const DROP_HOLD_S: f32 = 1.4;
/// Retombée d'un coup de kick sur la déchirure : `1 / 3,2` de Sillage.
const KICK_DECAY_S: f32 = 0.31;

/// L'immeuble ERROR.
#[derive(Debug)]
pub struct ErrorBuilding {
    rng: Rand,
    burst: Burst,
    word: usize,
    kicks: u32,
    /// Déchirure rendue par le limiteur, 0..1.
    glitch: f32,
    sign_offset: Vec2,
    beam_from: Vec3,
    beam_to: Vec3,
    beam_amount: f32,
    mood: Vec3,
}

impl Default for ErrorBuilding {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorBuilding {
    /// L'immeuble ERROR.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rng: Rand::new(0x0404_E220),
            burst: Burst::default(),
            word: 0,
            kicks: 0,
            glitch: 0.0,
            sign_offset: Vec2::ZERO,
            beam_from: Vec3::new(0.0, SIGN_Y, 1.0),
            beam_to: Vec3::ZERO,
            beam_amount: 0.0,
            mood: Vec3::ZERO,
        }
    }

    /// Le mot affiché (rang dans [`ERROR_WORDS`]).
    #[must_use]
    pub fn word(&self) -> usize {
        self.word
    }

    /// La déchirure courante, 0..1 (celle du limiteur).
    #[must_use]
    pub fn glitch(&self) -> f32 {
        self.glitch
    }

    fn next_word(&mut self) {
        self.kicks = 0;
        let step = 1 + (self.rng.unit() * (ERROR_WORDS.len() - 1) as f64) as usize;
        self.word = (self.word + step) % ERROR_WORDS.len();
    }

    fn word_params(&self) -> Vec4 {
        let span = spans()[self.word];
        Vec4::new(self.word as f32, span.from, span.to, span.letters as f32)
    }

    /// Les sept plans du panneau, la couche de chaque sommet dans `uv1.x`.
    fn sign_mesh() -> MeshData {
        let mut mesh = MeshData::default();
        for i in 0..LAYERS {
            mesh.push_quad(Vec3::new(0.0, 0.0, -(i as f32) * LAYER_GAP), Vec2::new(SIGN_W, SIGN_H));
            let layer = i as f32 / (LAYERS - 1) as f32;
            mesh.uv1.extend([[layer, 0.0]; 4]);
        }
        mesh
    }
}

impl Landmark for ErrorBuilding {
    fn kind(&self) -> LandmarkKind {
        LandmarkKind::Error
    }

    fn span(&self) -> f32 {
        SPAN
    }

    /// Une enseigne-phare : elle apparaît bien plus loin que les autres immeubles.
    fn lead(&self) -> Option<f32> {
        Some(620.0)
    }

    fn body(&self) -> Option<BodySpec> {
        Some(BodySpec {
            height: HEIGHT,
            depth: DEPTH,
            seed: 404,
        })
    }

    /// Le rouge est la couleur de l'immeuble, quoi que la ville ait tiré au sort.
    fn body_tint(&self) -> Option<SceneColor> {
        Some(SceneColor::Alarm)
    }

    fn parts(&self) -> Vec<PartSpec> {
        vec![
            // Le panneau tremble de ±1,5 m en x et ±0,5 m en y : l'enveloppe en tient compte.
            PartSpec::new(
                "panneau",
                MeshSpec::Custom(Box::new(Self::sign_mesh())),
                Look::ErrorSign,
            )
            .with_map(MapId::ErrorAtlas)
            .with_bounds(
                Vec3::new(-SIGN_W / 2.0 - 2.0, -SIGN_H / 2.0 - 1.0, -(LAYERS as f32) * LAYER_GAP),
                Vec3::new(SIGN_W / 2.0 + 2.0, SIGN_H / 2.0 + 1.0, 1.0),
            ),
            PartSpec::new("mât gauche", MeshSpec::Cube, Look::Iron),
            PartSpec::new("mât droit", MeshSpec::Cube, Look::Iron),
            PartSpec::new("traverse", MeshSpec::Cube, Look::Iron),
            PartSpec::new("façade", MeshSpec::Quad { w: SPAN, h: HEIGHT }, Look::ErrorFacade),
            PartSpec::new("faisceau", MeshSpec::Custom(Box::new(MeshData::pyramid())), Look::Beam),
        ]
    }

    fn show(&mut self, _place: &Placement) {
        self.kicks = 0;
        self.word = 0;
        self.glitch = 0.0;
        self.beam_amount = 0.0;
        self.burst.reset();
    }

    fn hide(&mut self) {
        self.beam_amount = 0.0;
        self.burst.reset();
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        let permitted = frame.policy.permitted();
        self.burst.step(frame.dt, permitted);
        if frame.beat.kick {
            // Plus la frappe est forte, plus la distorsion monte (si le limiteur l'accorde).
            let strength = clamp01(0.2 + frame.beat.bass * 1.1);
            self.burst.trigger_scaled(strength, KICK_DECAY_S, frame.time);
            self.kicks += 1;
            if self.kicks >= WORD_EVERY_KICKS {
                self.next_word();
            }
        }
        if frame.drive.drop {
            self.burst.trigger(DROP_HOLD_S, frame.time);
            self.next_word();
        }
        let g = self.burst.level();
        self.glitch = g;

        // Le panneau tremble sur place quand il déraille (sinusoïdes : un mouvement, pas un scintillement).
        let t = frame.time as f32;
        let shake = g * g;
        self.sign_offset = Vec2::new(
            ((t * 41.0).sin() + (t * 23.0).sin()) * 0.7 * shake,
            ((t * 37.0).sin() + (t * 19.0).sin()) * 0.25 * shake,
        );

        // Le mot du sol suit la voiture, tant qu'elle est dans le champ du panneau.
        let place = ctx.place;
        let dz = (frame.car.z - place.z).abs() as f32;
        let near = 1.0 - smoothstep(110.0, 230.0, dz);
        let blink = 1.0 - 0.3 * g;
        let cx = (frame.car.x as f32).clamp(-4.0, 4.0);
        let cz = frame.car.z - DECAL_LEAD;
        let amount = near * blink * (0.55 + g * 0.45);
        ctx.field.set_decal_box(cx, cz, DECAL_HALF_W, DECAL_HALF_D, amount);
        ctx.field.set_decal_word(self.word_params());
        ctx.field.set_decal_glitch(g);
        self.beam_from = Vec3::new(0.0, SIGN_Y, 1.0);
        self.beam_to = place.local(DVec3::new(f64::from(cx), 0.0, cz));
        self.beam_amount = near * (0.5 + g * 1.2);

        // Le glitch déborde sur la rue et les façades voisines.
        let alarm = frame.colors.get(SceneColor::Alarm);
        let zone = g * (1.0 - smoothstep(70.0, 210.0, dz));
        ctx.field.set_zone(place.z, 95.0, zone, alarm);
        // Deux sources réelles : le panneau, et un halo bas qui lèche la rue.
        ctx.light(
            0,
            Vec3::new(0.0, SIGN_Y, 10.0),
            150.0,
            alarm,
            1.0 + g * 2.2 + frame.beat.pulse * 0.6,
        );
        ctx.light(1, Vec3::new(0.0, 8.0, 9.0), 80.0, alarm, 0.6 + frame.beat.pulse * 1.4);
        self.mood = alarm;
    }

    fn paint(&self, _frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let [sign, mast_l, mast_r, bar, facade, beam] = out else {
            return;
        };
        let g = Vec4::new(self.glitch, 0.0, 0.0, 0.0);
        let base = PartPaint {
            mood: self.mood,
            a: g,
            ..PartPaint::default()
        };
        *sign = PartPaint {
            transform: Transform::from_xyz(self.sign_offset.x, SIGN_Y + self.sign_offset.y, -3.0),
            b: self.word_params(),
            ..base
        };
        // Mâts et traverse qui portent le panneau. Les mâts partent du sol : tant que l'immeuble n'a pas fini de pousser,
        // l'enseigne tient sur un pylône.
        let post_h = SIGN_Y - SIGN_H / 2.0;
        for (slot, x) in [(mast_l, -22.0), (mast_r, 22.0)] {
            *slot = PartPaint {
                transform: Transform::from_xyz(x, post_h / 2.0, -3.6).with_scale(Vec3::new(1.6, post_h, 1.6)),
                ..base
            };
        }
        *bar = PartPaint {
            transform: Transform::from_xyz(0.0, post_h - 0.2, -3.6).with_scale(Vec3::new(SIGN_W - 4.0, 1.2, 1.2)),
            ..base
        };
        *facade = PartPaint {
            transform: Transform::from_xyz(0.0, HEIGHT / 2.0, 0.15),
            ..base
        };
        let direction = self.beam_to - self.beam_from;
        let distance = direction.length().max(1e-3);
        *beam = PartPaint {
            visible: self.beam_amount > 0.001,
            // La pyramide pointe vers +Z : le repère de Bevy regarde vers −Z, on vise le point symétrique.
            transform: Transform::from_translation(self.beam_from)
                .looking_at(self.beam_from - direction, Vec3::Y)
                .with_scale(Vec3::new(DECAL_HALF_W * 1.05, DECAL_HALF_D * 2.6, distance)),
            a: Vec4::new(self.beam_amount, 0.0, 0.0, 0.0),
            mood: self.mood,
            ..PartPaint::default()
        };
    }
}

#[cfg(test)]
mod tests;
