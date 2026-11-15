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
    idle: Option<Res<cf_render::SceneIdle>>,
    run: Option<Res<crate::run::RunStatus>>,
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
    // Décor au repos : ni projecteurs de tours ni projecteurs épinglés par les événements.
    // Ni pendant que le quartier se lève (ouverture, retour d'une passation), ni une fois qu'il s'enfonce : un
    // projecteur n'a de sens que sur une tour debout.
    let towers_down = run.is_some_and(|r| !towers_stand(r.0.phase, r.0.stages.build, r.0.stages.collapse));
    let idle = idle.is_some_and(|i| i.0) || towers_down;
    let anchors = match world.as_deref().filter(|w| w.has_city() && !idle) {
        Some(world) => window.refresh(car.0.z, world.scenery()),
        None => &[],
    };
    let pins = traffic.as_deref().filter(|_| !idle).map_or(&[][..], |t| t.scene.pins());
    field.update(&BeamInput {
        dt_s,
        time_s: now_s,
        beat: features.beat,
        mid: features.mid,
        kick: pulse.pulse(),
        onset: features.onset,
        anchors,
        pins,
        cycle,
    });
}

/// Les tours sont-elles debout ? Toujours pendant le show ; jamais en attente ni pendant l'ouverture ; pendant une
/// clôture ou une passation, seulement tant qu'elles n'ont pas commencé à s'enfoncer.
#[must_use]
pub(crate) fn towers_stand(phase: cf_render::Phase, build: f32, collapse: f32) -> bool {
    use cf_render::Phase;
    match phase {
        Phase::Running => true,
        Phase::Waiting | Phase::Starting => false,
        Phase::Stopping | Phase::Handover => build >= 1.0 && collapse < 0.15,
    }
}

#[cfg(test)]
mod tests {
    use super::towers_stand;
    use cf_render::Phase;

    #[test]
    fn no_beam_before_the_district_is_up_nor_once_it_sinks() {
        assert!(towers_stand(Phase::Running, 1.0, 0.0));
        assert!(!towers_stand(Phase::Waiting, 0.0, 0.0));
        assert!(
            !towers_stand(Phase::Starting, 0.9, 0.0),
            "pas pendant que les tours montent"
        );
        assert!(towers_stand(Phase::Stopping, 1.0, 0.0));
        assert!(!towers_stand(Phase::Stopping, 1.0, 0.5));
        assert!(
            !towers_stand(Phase::Handover, 0.4, 0.0),
            "ni pendant le retour du quartier"
        );
    }
}
