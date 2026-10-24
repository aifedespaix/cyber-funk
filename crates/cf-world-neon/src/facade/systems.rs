//! Le système des façades : remplir le matériau d'après l'image.

use bevy::math::Vec4;
use bevy::prelude::*;
use cf_render::palette::LinearRgb;
use cf_render::{AudioFeatures, SceneColor, ScenePalette};

use super::material::FacadeParams;
use super::{FacadeLook, FacadeMaterial};
use crate::road::RoadLook;

/// Période, en s, au bout de laquelle le temps du shader repart de zéro : 40 min.
///
/// Le temps est envoyé en `f32` : sans borne, `fract(t × 1,5 + …)` perdrait sa précision au fil d'un set de plusieurs
/// heures. 2 400 s × 1,5 = 3 600 : un nombre entier de périodes du clignotement des fenêtres, qui revient donc à
/// l'identique. Seuls les filets d'eau (pluie), dont la vitesse varie d'un filet à l'autre, repartent d'un coup.
const TIME_WRAP_S: f64 = 2400.0;

/// Ce que le shader lit de l'image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameInputs {
    /// Temps écoulé, en s.
    pub time_s: f64,
    /// Pouls du kick, 0 à 1 (`b.pulse`).
    pub pulse: f32,
    /// Énergie des basses, 0 à 1.
    pub bass: f32,
    /// Médiums, 0 à 1.
    pub mid: f32,
    /// Aigus, 0 à 1.
    pub high: f32,
    /// Temps musical continu (`b.beat`) : le numéro du temps plus une montée douce de 0 à 1.
    pub beat: f64,
}

impl Default for FrameInputs {
    fn default() -> Self {
        Self { time_s: 0.0, pulse: 0.0, bass: 0.0, mid: 0.0, high: 0.0, beat: 0.0 }
    }
}

fn vec4(color: LinearRgb) -> Vec4 {
    Vec4::new(color.r, color.g, color.b, 1.0)
}

/// Les uniformes de l'image (`Scenery.update` de Sillage, plus le bloc de palette).
#[must_use]
pub fn build_params(palette: &ScenePalette, look: &FacadeLook, wet: f32, rain: f32, frame: &FrameInputs) -> FacadeParams {
    let get = |color| palette.get(color);
    // `uMist` de Sillage : `velvet` mélangé de 35 % au violet, assombri de 10 %.
    let mist = get(SceneColor::Velvet).lerp(get(SceneColor::Violet), 0.35);
    let mist = LinearRgb { r: mist.r * 0.9, g: mist.g * 0.9, b: mist.b * 0.9 };
    FacadeParams {
        ink: vec4(get(SceneColor::Ink)),
        ice: vec4(get(SceneColor::Ice)),
        amber: vec4(get(SceneColor::Amber)),
        haze: vec4(get(SceneColor::Violet)),
        mist: vec4(mist),
        scan: vec4(get(SceneColor::Mint)),
        cycle: palette.prismatic_cycle().map(vec4),
        frame: Vec4::new(
            (frame.time_s % TIME_WRAP_S) as f32,
            frame.pulse,
            frame.bass,
            frame.mid,
        ),
        audio: Vec4::new(frame.high, frame.beat as f32, look.build.clamp(0.0, 1.0), look.flash.clamp(0.0, 1.0)),
        weather: Vec4::new(wet.clamp(0.0, 1.0), rain.clamp(0.0, 1.0), look.win_pulse.clamp(0.0, 1.0), 0.0),
    }
}

/// `CfSet::Present` : remplit le matériau (temps, musique, météo, palette). Le matériau n'est touché que si quelque chose a
/// changé : Bevy ne re-prépare pas un asset qu'on n'a pas écrit.
pub(super) fn update_material(
    time: Res<Time<Real>>,
    features: Option<Res<AudioFeatures>>,
    palette: Res<ScenePalette>,
    look: Res<FacadeLook>,
    road: Res<RoadLook>,
    materials: Option<ResMut<Assets<FacadeMaterial>>>,
    scenery: Option<Res<crate::scenery::SceneryAssets>>,
) {
    let (Some(mut materials), Some(scenery)) = (materials, scenery) else { return };
    let frame = features.as_deref().map_or_else(FrameInputs::default, |f| FrameInputs {
        time_s: 0.0,
        pulse: f.0.pulse,
        bass: f.0.bass,
        mid: f.0.mid,
        high: f.0.high,
        beat: f.0.beat,
    });
    let frame = FrameInputs { time_s: look.clock_s.unwrap_or_else(|| time.elapsed_secs_f64()), ..frame };
    let params = build_params(&palette, &look, road.wet, road.rain, &frame);
    if materials.get(&scenery.material).is_some_and(|m| m.params == params) {
        return;
    }
    if let Some(mut material) = materials.get_mut(&scenery.material) {
        material.params = params;
    }
}
