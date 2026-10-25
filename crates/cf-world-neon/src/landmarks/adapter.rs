//! L'adaptateur entre la scène des immeubles-vitrines et l'événement `landmark` du trafic (J7-05).
//!
//! [`LandmarkEvent`](crate::traffic::grand::LandmarkEvent) sait tout du passage : il pose l'immeuble devant la voiture,
//! dégage la ville autour, publie le **sujet `Landmark`** du réalisateur et la structure à esquiver, passe la musique à chaque
//! image et le range une fois dépassé. Il lui faut un [`Landmark`](crate::traffic::grand::Landmark) ; [`StageLandmark`] en
//! est un, qui n'a pas de logique à lui : il **délègue à la scène** ([`LandmarkStage`]) l'immeuble tiré, et c'est la couche de
//! rendu ([`super::render`]) qui dessine ce que la scène a écrit.
//!
//! Le tirage (jamais l'un des deux derniers, les plus calmes en mode calme, pondéré par la fraîcheur du spectacle) a lieu à la
//! fabrication de l'adaptateur, une fois par passage, jamais par image.
//!
//! # Règle 7
//!
//! Le pouls que lisent les immeubles est celui que le limiteur de flashs du trafic accorde à toute la ville
//! (`SceneFx::kick`) : les immeubles et les façades partagent un seul budget de trois départs par seconde. La permission
//! des éclats propres à chaque immeuble (éruption, déchirure…) est `SceneFx::flashes`.

use super::frame::{Drive, FlashPolicy, LandmarkBeat, LandmarkFrame};
use super::landmark::{LandmarkKind, Placement};
use super::plugin::LandmarkHandle;
use super::stage::LandmarkStage;
use crate::facade::tint_rank;
use crate::traffic::grand::{Landmark, LandmarkFrame as TrafficLandmarkFrame, RoadBand};
use crate::traffic::{EventCtx, TrafficBeat};
use bevy::color::LinearRgba;

/// Un immeuble-vitrine de la scène, vu de l'événement du trafic.
#[derive(Debug)]
pub struct StageLandmark {
    handle: LandmarkHandle,
    kind: LandmarkKind,
}

impl StageLandmark {
    /// L'immeuble `kind` de la scène.
    #[must_use]
    pub fn of(handle: &LandmarkHandle, kind: LandmarkKind) -> Self {
        Self {
            handle: handle.clone(),
            kind,
        }
    }

    /// Tire l'immeuble du prochain passage : celui qu'un ordre de mise au point demande, sinon un tirage (`chooseLandmark`).
    #[must_use]
    pub fn choose(handle: &LandmarkHandle, ctx: &mut EventCtx<'_>) -> Self {
        let mut stage = handle.lock();
        let kind = stage
            .take_forced()
            .unwrap_or_else(|| stage.choose(ctx.rng, ctx.frame.calm, ctx.frame.time));
        drop(stage);
        Self::of(handle, kind)
    }

    /// Lequel.
    #[must_use]
    pub fn kind(&self) -> LandmarkKind {
        self.kind
    }

    /// Un pas, sans `EventCtx` : `policy` est la politique de flashs, `shared_pulse` le pouls accordé à toute la ville
    /// (à défaut, celui du limiteur de la scène). C'est ce qu'appelle [`Landmark::step`] ; les exemples et les tests qui n'ont
    /// pas de directeur l'appellent directement.
    pub fn drive(&mut self, frame: &TrafficLandmarkFrame, policy: FlashPolicy, shared_pulse: Option<f32>) {
        let mut stage = self.handle.lock();
        let mut f = convert(&stage, frame, policy);
        stage.step(&mut f, shared_pulse);
    }
}

/// L'image de l'événement du trafic devient celle d'un immeuble.
fn convert(stage: &LandmarkStage, frame: &TrafficLandmarkFrame, policy: FlashPolicy) -> LandmarkFrame {
    let TrafficBeat {
        kick,
        bass,
        mid,
        high,
        loud,
        pulse,
        count,
        beat,
        bpm,
    } = frame.beat;
    LandmarkFrame {
        time: frame.time,
        dt: frame.dt,
        beat: LandmarkBeat {
            kick,
            bass,
            mid,
            high,
            loud,
            // Le pouls brut : la scène le remplace par celui du limiteur avant de le donner à l'immeuble.
            pulse,
            count,
            bpm,
            beat: beat as f32,
        },
        drive: Drive {
            drop: frame.drive.drop,
            intensity: frame.drive.intensity,
            section: frame.drive.section,
            mood: stage.inputs.mood,
            // Comme `TrafficDrive` : la fin de titre n'est branchée nulle part (Sillage la lisait sur la durée du fichier).
            outro: false,
        },
        car: frame.car,
        calm: frame.calm,
        phase: frame.phase,
        policy,
        colors: stage.inputs.colors,
    }
}

impl Landmark for StageLandmark {
    fn span(&self) -> f32 {
        self.handle.lock().landmark(self.kind).span()
    }

    fn lead(&self) -> Option<f32> {
        self.handle.lock().landmark(self.kind).lead()
    }

    fn spans_road(&self) -> bool {
        self.handle.lock().landmark(self.kind).spans_road()
    }

    fn road_band(&self) -> Option<RoadBand> {
        self.handle.lock().landmark(self.kind).road_band().map(|band| RoadBand {
            half_depth: f64::from(band.half_depth),
            lo: f64::from(band.lo),
            hi: f64::from(band.hi),
        })
    }

    fn anchor_x(&self) -> f32 {
        let stage = self.handle.lock();
        stage
            .active()
            .filter(|a| a.kind == self.kind)
            .map_or(0.0, |a| a.place.root_position().x as f32)
    }

    fn show(&mut self, z: f64, side: i8, _tint: LinearRgba) {
        let mut stage = self.handle.lock();
        let landmark = stage.landmark(self.kind);
        let spans_road = landmark.spans_road();
        // Tous les immeubles à corps imposent leur couleur (le rouge d'ERROR, le vert du mur de code…) : la teinte que la
        // ville a tirée n'a de sens que pour ceux qui n'ont pas de corps (arche, mecha), qui ne la lisent pas.
        let tint = landmark.body_tint().and_then(tint_rank).map_or(0, |rank| rank as u8);
        stage.show(
            self.kind,
            Placement {
                z,
                side,
                tint,
                scale: 1.0,
                spans_road,
            },
        );
    }

    fn set_scale(&mut self, scale: f32) {
        self.handle.lock().set_scale(scale);
    }

    fn step(&mut self, frame: &TrafficLandmarkFrame, ctx: &mut EventCtx<'_>) {
        let fx = &ctx.scene.fx;
        let policy = FlashPolicy {
            flashes: fx.flashes,
            breathing: false,
        };
        let pulse = fx.kick;
        self.drive(frame, policy, Some(pulse));
    }

    fn hide(&mut self) {
        let mut stage = self.handle.lock();
        if stage.active().is_some_and(|a| a.kind == self.kind) {
            stage.hide();
        }
    }
}

#[cfg(test)]
mod tests;
