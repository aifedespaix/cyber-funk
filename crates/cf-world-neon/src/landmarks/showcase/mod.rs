//! Les dix immeubles-vitrines à façade (J7-09, `FacadeLandmark` de `facade.ts` et `landmarks/showcase/`) : l'orgue, l'aquarium,
//! la serre, le phare, l'horloge, la glace, la tour radio, la pagode, la manette et l'hôtel de verre.
//!
//! Un même type, [`Showcase`], les porte tous : un bloc de ville (le corps), un plan de façade dont le fragment est un shader
//! (`landmark_show_<nom>.wesl`), des lumières de ville réparties sur la hauteur, et, pour trois d'entre eux, une construction
//! annexe ([`extras`]) : le faisceau du phare, les ondes de la tour radio, les blocs de la pagode.
//!
//! Écarts avec Sillage (règle 7 de `CLAUDE.md`, voir la PR) :
//!
//! - `uDrop` est le niveau d'un limiteur de flashs ([`Burst`]) et non un 1 brut : les chambres de l'hôtel, le blanc de la
//!   manette et les rosaces ne s'allument plus que si les flashs sont permis, trois fois par seconde au plus ;
//! - les horloges qui multipliaient `time` par une énergie audio (banc de l'aquarium, vague de la serre, rouages et aiguilles de
//!   l'horloge) sont intégrées côté CPU : le produit téléportait les motifs à chaque variation de l'énergie ;
//! - la pagode utilise un matériau léger (volume sombre, fenêtres, brouillard) à la place du matériau des gratte-ciel.

pub mod extras;
pub mod spec;

use bevy::math::{Quat, Vec3};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use self::extras::{BEAM_REACH, LAMP, RADIO_TOP, TILT, WAVES};
use self::spec::{Extra, FacadeSpec, spec_of};
use super::flash::Burst;
use super::frame::LandmarkFrame;
use super::landmark::{Landmark, LandmarkKind, Placement, StepCtx};
use super::math::hash;
use super::part::{BodySpec, Look, MeshSpec, PartPaint, PartSpec};

/// Les horloges intégrées repartent de zéro après ce nombre de secondes-unités (la précision d'un `f32` tient à 65 536).
const CLOCK_WRAP: f64 = 65_536.0;
/// Les lumières de façade sont posées à 12 m devant elle, comme `animate` de `FacadeLandmark`.
const LIGHT_Z: f32 = 12.0;

/// Un immeuble-vitrine à façade.
#[derive(Debug)]
pub struct Showcase {
    spec: &'static FacadeSpec,
    burst: Burst,
    /// Niveau de drop borné par le limiteur, 0..1 (`uDrop`, `uBlast`).
    level: f32,
    /// Première horloge intégrée (`b.w` du shader).
    clock_b: f64,
    /// Seconde horloge intégrée (`c.w` du shader).
    clock_c: f64,
    /// Azimut du faisceau du phare, rad.
    angle: f32,
    /// L'immeuble vient d'être posé : le phare choisit son azimut de départ au premier pas.
    fresh: bool,
    colors: [Vec3; 4],
    tint: Vec3,
    /// Puissance du faisceau (`uPower`).
    beam_power: f32,
    sun: Vec3,
}

impl Showcase {
    /// L'immeuble de ce type, ou `None` si ce n'est pas une vitrine à façade.
    #[must_use]
    pub fn new(kind: LandmarkKind) -> Option<Self> {
        spec_of(kind).map(|spec| Self {
            spec,
            burst: Burst::default(),
            level: 0.0,
            clock_b: 0.0,
            clock_c: 0.0,
            angle: 0.0,
            fresh: true,
            colors: [Vec3::ONE; 4],
            tint: Vec3::ONE,
            beam_power: 1.0,
            sun: Vec3::ONE,
        })
    }

    /// Le niveau de drop courant, 0..1.
    #[must_use]
    pub fn drop_level(&self) -> f32 {
        self.level
    }

    /// L'azimut du faisceau du phare, rad.
    #[must_use]
    pub fn beam_angle(&self) -> f32 {
        self.angle
    }

    fn advance(clock: &mut f64, rate: f32, dt: f32) {
        *clock = (*clock + f64::from(rate * dt)) % CLOCK_WRAP;
    }
}

impl Landmark for Showcase {
    fn kind(&self) -> LandmarkKind {
        self.spec.kind
    }

    fn span(&self) -> f32 {
        self.spec.span
    }

    fn body(&self) -> Option<BodySpec> {
        self.spec.body.then_some(BodySpec {
            height: self.spec.height,
            depth: self.spec.depth,
            seed: self.spec.seed,
        })
    }

    fn body_tint(&self) -> Option<SceneColor> {
        Some(self.spec.tint)
    }

    fn parts(&self) -> Vec<PartSpec> {
        let mut parts = vec![PartSpec::new(
            "façade",
            MeshSpec::Quad {
                w: self.spec.span,
                h: self.spec.height,
            },
            self.spec.look,
        )];
        match self.spec.extra {
            Extra::None => {}
            Extra::Beam => parts.push(PartSpec::new(
                "faisceau",
                MeshSpec::Custom(Box::new(extras::beam_mesh())),
                Look::LighthouseBeam,
            )),
            Extra::Waves => parts.push(PartSpec::new(
                "ondes",
                MeshSpec::Quad { w: WAVES, h: WAVES },
                Look::RadioWaves,
            )),
            Extra::Blocks => parts.push(PartSpec::new(
                "blocs",
                MeshSpec::Custom(Box::new(extras::pagoda_mesh())),
                Look::PagodaBlocks,
            )),
        }
        parts
    }

    fn show(&mut self, place: &Placement) {
        self.burst.reset();
        self.level = 0.0;
        self.fresh = true;
        // Le phare démarre à un azimut qui dépend de l'endroit : deux passages ne balaient pas au même moment.
        self.angle = hash(place.z.abs() as u32, 77) * std::f32::consts::TAU;
    }

    fn hide(&mut self) {
        self.burst.reset();
        self.level = 0.0;
    }

    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>) {
        let spec = self.spec;
        let drive = &frame.drive;
        self.colors = spec.colors.map(|color| frame.colors.get(color));
        self.tint = frame.colors.get(spec.tint);
        self.sun = frame.colors.get(SceneColor::Sun);
        self.burst.step(frame.dt, frame.policy.permitted());
        if drive.drop {
            self.burst.trigger(spec.drop_decay_s, frame.time);
        }
        self.level = self.burst.level();

        match spec.kind {
            LandmarkKind::Aquarium => Self::advance(
                &mut self.clock_b,
                1.0 + frame.beat.bass * 2.5 + drive.intensity,
                frame.dt,
            ),
            LandmarkKind::Greenhouse => Self::advance(&mut self.clock_b, 1.2 + frame.beat.bass * 2.0, frame.dt),
            LandmarkKind::Clock => {
                Self::advance(&mut self.clock_b, 0.25 + drive.intensity * 0.5, frame.dt);
                Self::advance(&mut self.clock_c, 1.0 + self.level * 6.0, frame.dt);
            }
            _ => {}
        }

        let power = 0.9 + drive.intensity * 1.1 + frame.beat.pulse * 1.3 + self.level * 1.5;
        for (i, fraction) in spec.lights.iter().enumerate() {
            ctx.light(
                i,
                Vec3::new(0.0, spec.height * fraction, LIGHT_Z),
                spec.light_radius,
                self.tint,
                power,
            );
        }

        if spec.extra == Extra::Beam {
            if self.fresh && frame.calm {
                self.angle = 0.0;
            }
            self.fresh = false;
            let speed = (if frame.calm { 0.35 } else { 0.8 } + drive.intensity * 0.7) * (1.0 + 2.0 * self.level);
            self.angle = (self.angle + frame.dt * speed) % std::f32::consts::TAU;
            self.beam_power = 0.8 + frame.beat.pulse * 0.5 + drive.intensity * 0.5;
            // Le bout du faisceau éclaire la ville : une lumière qui suit le balayage.
            let tip = Vec3::new(
                (-self.angle).cos() * BEAM_REACH,
                LAMP - 8.0,
                -spec.span * 0.4 + (-self.angle).sin() * BEAM_REACH,
            );
            ctx.light(2, tip, 80.0, self.sun, 1.4 + frame.beat.pulse);
        }
    }

    fn paint(&self, frame: &LandmarkFrame, out: &mut [PartPaint]) {
        let spec = self.spec;
        let [a, b, c, d] = self.colors;
        if let Some(facade) = out.first_mut() {
            *facade = PartPaint {
                transform: Transform::from_xyz(0.0, spec.height / 2.0, 0.2),
                mood: self.tint,
                a: a.extend(self.level),
                b: b.extend(self.clock_b as f32),
                c: c.extend(self.clock_c as f32),
                d: d.extend(0.0),
                ..PartPaint::default()
            };
        }
        let Some(extra) = out.get_mut(1) else {
            return;
        };
        *extra = match spec.extra {
            Extra::None => return,
            Extra::Beam => PartPaint {
                transform: Transform {
                    translation: Vec3::new(0.0, LAMP + 1.5, -spec.span * 0.4),
                    rotation: Quat::from_rotation_y(self.angle) * Quat::from_rotation_x(TILT),
                    ..Transform::IDENTITY
                },
                mood: self.tint,
                a: self.sun.extend(self.beam_power),
                ..PartPaint::default()
            },
            Extra::Waves => {
                // La teinte des ondes va de la glace au rose avec l'intensité du morceau.
                let tone = a.lerp(b, frame.drive.intensity.clamp(0.0, 1.0));
                PartPaint {
                    transform: Transform::from_xyz(0.0, RADIO_TOP, 4.0),
                    mood: self.tint,
                    a: tone.extend(self.level),
                    ..PartPaint::default()
                }
            }
            Extra::Blocks => PartPaint {
                transform: Transform::IDENTITY,
                mood: self.tint,
                a: a.extend(0.0),
                ..PartPaint::default()
            },
        };
    }
}

#[cfg(test)]
mod tests;
