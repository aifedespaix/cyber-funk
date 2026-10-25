//! Le système qui place les projecteurs : lit l'audio, le spectacle, la ville et les projecteurs épinglés du trafic.

use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, CurrentShowFx, ScenePalette, StreamFocus};

use super::Beams;
use super::field::BeamInput;
use crate::traffic::TrafficDirector;
use crate::world::NeonWorld;

/// `CfSet::Simulate`, après le trafic : le pouls des kicks passe par le limiteur, puis les projecteurs avancent d'une image.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn step_beams(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show_fx: Option<Res<CurrentShowFx>>,
    palette: Option<Res<ScenePalette>>,
    world: Option<Res<NeonWorld>>,
    traffic: Option<Res<TrafficDirector>>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut beams: ResMut<Beams>,
) {
    let Ok(car) = car.single() else { return };
    let features = features.map_or(cf_analysis::AudioFeatures::NEUTRAL, |f| f.0);
    let policy = show_fx.map(|s| (s.0.flashes, s.0.breathing));
    let (dt_s, now_s) = (time.delta_secs(), time.elapsed_secs_f64());
    let cycle = palette.map_or([[1.0; 3]; 6], |p| p.prismatic_cycle().map(|c| [c.r, c.g, c.b]));
    let Beams { field, pulse, window } = &mut *beams;
    pulse.update(dt_s, now_s, policy, features.pulse);
    // Sans ville (route seule), pas de tours donc pas de balayeurs.
    let anchors = match world.as_deref().filter(|w| w.has_city()) {
        Some(world) => window.refresh(car.0.z, world.scenery()),
        None => &[],
    };
    let pins = traffic.as_deref().map_or(&[][..], |t| t.scene.pins());
    field.update(&BeamInput {
        dt_s,
        time_s: now_s,
        beat: features.beat,
        mid: features.mid,
        kick: pulse.pulse(),
        anchors,
        pins,
        cycle,
    });
}
