//! Applique la séquence au monde : le plugin et le système `drive_run`.
//!
//! ```text
//! RunOrder ─▶ RunSequencer ─▶ RunOutput ─┬▶ RunPhase / RunStatus  (l'opérateur et les autres systèmes du monde le lisent)
//!                                        ├▶ RoadLook.draw, FacadeLook.build, SkyLook.reveal, DjSignIntro
//!                                        ├▶ CarControls (présence, phares), PilotSettings (vitesse imposée ou plafonnée)
//!                                        ├▶ ShotOrder (plans `waiting`, `opening`, `closing`, `handover`) et StagingFeed
//!                                        └▶ front des fenêtres (comme la pré-intro)
//! ```
//!
//! Comme la pré-intro, le séquenceur n'écrit que quand il a la main : il rend tout à 1 une fois, en entrant dans
//! `Running`, puis ne touche plus à rien (d'autres — événements, outils — règlent alors ces valeurs).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use cf_cinema::{ShotOrder, TransitionKind};
use cf_render::{
    AudioCommand, AudioFeatures, AudioSequenceGain, CfSet, ControlAction, CurrentShow, Phase, RunHandover, RunOrder,
    RunPhase, SceneIdle, ShowSheet, SimControl, Staging, StagingFeed, StagingView,
};

use super::machine::{RunOutput, RunSequencer};
use super::timeline::{CameraPlan, START_S, STOP_S, city_seconds};
use crate::car::rig::CarState;
use crate::intro::{self, Owned, Targets, set};

/// Marque que le séquenceur est installé : la pré-intro de niveau 0 s'efface devant lui.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct RunDriven;

/// Ce que la séquence a décidé cette image : les systèmes de la voiture, de la caméra et de l'audio le lisent.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RunStatus(pub RunOutput);

/// Installe le séquenceur. Le binaire l'ajoute ; sans lui, [`RunPhase`] reste à `Running` et rien ne change.
#[derive(Debug, Default, Clone, Copy)]
pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        let sequencer = RunSequencer::new();
        let first = RunSequencer::new().update(None, 0.0, false);
        app.add_plugins(cf_render::RunPlugin)
            .add_message::<ShotOrder>()
            .add_message::<AudioCommand>()
            .add_message::<ControlAction>()
            .init_resource::<AudioSequenceGain>()
            .init_resource::<StagingFeed>()
            .insert_resource(RunDriven)
            .insert_resource(cf_render::RunSequenced)
            .insert_resource(Sequencer(sequencer))
            .insert_resource(RunPhase {
                phase: Phase::Waiting,
                progress: 0.0,
            })
            .insert_resource(RunStatus(first))
            // Avant la pré-intro (qui lit `RunDriven`) et avant l'enseigne.
            .add_systems(
                Update,
                drive_run
                    .in_set(CfSet::Simulate)
                    .before(crate::djsign::step_sign),
            );
    }
}

#[derive(Resource, Debug)]
struct Sequencer(RunSequencer);

/// Ce que `drive_run` lit et écrit du spectacle : la liste des DJs, le DJ actuel, le DJ visé par une passation, et la
/// voie par laquelle la passation est demandée au chef du spectacle.
#[derive(SystemParam)]
struct ShowLinks<'w> {
    show: Option<Res<'w, CurrentShow>>,
    sheet: Option<Res<'w, ShowSheet>>,
    settings: Option<Res<'w, crate::djsign::DjSignSettings>>,
    control: MessageWriter<'w, ControlAction>,
    handover: Option<ResMut<'w, RunHandover>>,
    idle: Option<ResMut<'w, SceneIdle>>,
    staging: ResMut<'w, StagingFeed>,
    gantry: Option<Res<'w, crate::gantry::GantryState>>,
}

/// `CfSet::Simulate` : lit l'ordre de l'opérateur, avance la séquence et l'applique au monde.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
fn drive_run(
    time: Res<Time>,
    mut orders: MessageReader<RunOrder>,
    features: Option<Res<AudioFeatures>>,
    sim: Option<ResMut<SimControl>>,
    mut shots: MessageWriter<ShotOrder>,
    mut audio: MessageWriter<AudioCommand>,
    mut gain: ResMut<AudioSequenceGain>,
    mut camera_held: Local<CameraHold>,
    mut links: ShowLinks,
    mut sequencer: ResMut<Sequencer>,
    cars: Query<&CarState>,
    mut phase: ResMut<RunPhase>,
    mut status: ResMut<RunStatus>,
    mut targets: Targets,
    mut owned: Local<Owned>,
) {
    let order = orders.read().last().copied();
    let before = sequencer.0.phase();
    let kick = features.is_some_and(|f| f.0.kick);
    // L'arrêt freine depuis la vitesse réelle de la voiture, pas depuis une vitesse supposée.
    if let Some(car) = cars.iter().next() {
        sequencer.0.set_speed(car.speed_mps);
    }
    // Où en est la liste des DJs : une passation n'a de sens qu'avec un autre DJ à qui passer.
    if let (Some(show), Some(sheet)) = (links.show.as_deref(), links.sheet.as_deref()) {
        sequencer.0.set_slot_info(show.0.slot, sheet.slots.len());
    }
    sequencer
        .0
        .set_reduce_motion(links.settings.as_deref().is_some_and(|s| s.calm));
    let out = sequencer.0.update(order, time.delta_secs(), kick);
    // La passation est demandée au chef du spectacle à l'entrée dans la phase : son fondu de palette est fini quand le
    // quartier est caché et que le portique arrive (`HANDOVER_FIRE_S`).
    if before != Phase::Handover
        && out.phase == Phase::Handover
        && let Some(actions) = out
            .handover_slot
            .and_then(|slot| ControlAction::go_to_dj(time.elapsed_secs_f64(), slot))
    {
        for action in actions {
            links.control.write(action);
        }
    }
    if let Some(handover) = links.handover.as_deref_mut() {
        set(&mut handover.0, out.handover_slot);
    }
    // Chaque `Démarrer` repart d'un show neuf : le chef du spectacle, l'horloge et la piste de démo sont remis à zéro.
    if before == Phase::Waiting
        && out.phase == Phase::Starting
        && let Some(mut sim) = sim
    {
        sim.request_reset();
    }
    // La phase publiée est celle que voit l'opérateur : à la dernière image d'une séquence, déjà la suivante.
    let published = RunPhase {
        phase: sequencer.0.phase(),
        progress: if sequencer.0.phase() == out.phase {
            out.progress
        } else {
            1.0
        },
    };
    // La musique suit la séquence ; à la fin de l'arrêt elle est mise en pause (le prochain `Démarrer` la relance du début).
    set(&mut gain.0, out.stages.music);
    if before == Phase::Stopping && sequencer.0.phase() == Phase::Waiting {
        audio.write(AudioCommand::DemoPause);
    }
    let wanted = match out.stages.camera {
        CameraPlan::Free => None,
        CameraPlan::Waiting => Some(WAITING_SHOT),
        CameraPlan::Opening => Some(OPENING_SHOT),
        CameraPlan::Closing => Some(CLOSING_SHOT),
        CameraPlan::Handover => Some(HANDOVER_SHOT),
    };
    direct_camera(wanted, &mut camera_held, &mut shots);
    let gantry = links.gantry.as_deref().and_then(crate::gantry::GantryState::placed);
    set(&mut links.staging.0, staging_view(&out, gantry));
    set(&mut *phase, published);
    set(&mut status.0, out);
    // Le décor est au repos tant que le quartier n'est pas là : attente, fin de l'arrêt, début du démarrage, passation
    // une fois le quartier replié. Rien ne s'y passe : ni brouillard, ni pubs, ni lasers, ni événements.
    if let Some(idle) = links.idle.as_deref_mut() {
        set(&mut idle.0, scene_is_idle(out.phase, out.stages.build));
    }

    // Hors séquence en cours, le séquenceur n'a la main qu'en attente ; en `Running` il rend tout une fois.
    let in_charge = out.phase != Phase::Running;
    if !in_charge && !owned.engaged {
        return;
    }
    owned.engaged = in_charge;
    let calm = links.settings.as_deref().is_some_and(|s| s.calm);
    apply(&out, &mut targets, &mut owned, calm);
}

/// Pose les étapes sur les cibles ; une cible absente est ignorée (exemples, tests).
fn apply(out: &RunOutput, targets: &mut Targets, owned: &mut Owned, calm: bool) {
    let s = out.stages;
    if let Some(r) = targets.road.as_deref_mut() {
        set(&mut r.draw, s.road);
    }
    if let Some(f) = targets.facade.as_deref_mut() {
        set(&mut f.build, s.build);
        set(&mut f.collapse, s.collapse);
    }
    if let Some(k) = targets.sky.as_deref_mut() {
        set(&mut k.reveal, s.sky);
    }
    if let Some(d) = targets.sign.as_deref_mut() {
        // Le nom du DJ est annoncé par le portique (`gantry/`). L'enseigne sur immeuble, elle, n'existe qu'avec son
        // immeuble : éteinte tant que le quartier n'est pas debout (sinon son nom flotte dans un ciel sans ville).
        let standing = s.build >= 1.0 && s.collapse <= 0.0;
        set(&mut d.0, if standing { None } else { Some(0.0) });
    }
    if let Some(c) = targets.controls.as_deref_mut() {
        set(&mut c.light_scale, s.lights);
        set(&mut c.presence, s.car_presence);
        set(&mut c.calm, calm);
    }
    if let Some(p) = targets.pilot.as_deref_mut() {
        set(&mut p.speed_cap_mps, out.speed_cap_mps.map(f64::from));
        set(&mut p.speed_forced_mps, out.speed_forced_mps.map(f64::from));
    }
    if let Some(t) = targets.traffic.as_deref_mut() {
        intro::apply_front(&mut t.scene.fx.power, out.front, owned);
    }
}

/// La séquence scénarisée vue par le réalisateur : l'ouverture (et la présentation du nouveau DJ d'une passation, qui
/// en reprend la fin), la clôture. `gantry` : où le portique est posé (x, z), s'il l'est.
#[must_use]
fn staging_view(out: &RunOutput, gantry: Option<(f64, f64)>) -> Option<StagingView> {
    let gantry = gantry.map(|(x, z)| bevy::math::DVec3::new(x, 0.0, z));
    let car_visible = out.stages.car_presence > 0.0;
    match out.phase {
        Phase::Starting => Some(StagingView {
            kind: Staging::Opening,
            t_s: out.t_s,
            duration_s: START_S,
            city_s: city_seconds(out.t_s),
            gantry,
            car_visible,
        }),
        Phase::Stopping => Some(StagingView {
            kind: Staging::Closing,
            t_s: out.t_s,
            duration_s: STOP_S,
            city_s: 0.0,
            gantry: None,
            car_visible,
        }),
        Phase::Handover => out.handover.map(|frame| {
            let marks = frame.variant.marks();
            StagingView {
                kind: Staging::Opening,
                t_s: frame.t_s,
                duration_s: marks.end,
                city_s: frame.t_s - (marks.gantry + super::handover::REBUILD_AFTER_GANTRY_S),
                gantry,
                car_visible,
            }
        }),
        Phase::Waiting | Phase::Running => None,
    }
}

/// Le plan voulu tient la caméra : l'attente, l'ouverture, la clôture, la passation. Sans plan voulu, le montage
/// reprend. Les plans scénarisés s'enchaînent en fondu de mouvement (ils se raccordent sur la même pose) ; seul le tout
/// premier plan du lancement, et la passation, entrent comme leur fiche le veut.
fn direct_camera(wanted: Option<&'static str>, held: &mut CameraHold, shots: &mut MessageWriter<ShotOrder>) {
    match (wanted, *held) {
        (Some(id), CameraHold::Unset) => {
            shots.write(ShotOrder::Hold(id));
            *held = CameraHold::On(id);
        }
        // La passation passe devant la voiture : une coupe franche, pas un survol.
        (Some(id), CameraHold::Free) if id == HANDOVER_SHOT => {
            shots.write(ShotOrder::Hold(id));
            *held = CameraHold::On(id);
        }
        // La clôture quitte un plan quelconque du show : elle revient derrière la voiture par le haut, sans la traverser.
        (Some(id), CameraHold::Free) if id == CLOSING_SHOT => {
            shots.write(ShotOrder::HoldWith(id, TransitionKind::Swoop));
            *held = CameraHold::On(id);
        }
        (Some(id), CameraHold::Free) => {
            shots.write(ShotOrder::HoldWith(id, TransitionKind::Blend));
            *held = CameraHold::On(id);
        }
        (Some(id), CameraHold::On(current)) if current != id => {
            shots.write(ShotOrder::HoldWith(id, TransitionKind::Blend));
            *held = CameraHold::On(id);
        }
        (None, CameraHold::On(_)) => {
            shots.write(ShotOrder::Release);
            *held = CameraHold::Free;
        }
        _ => {}
    }
}

/// Ce que le monde impose à la caméra.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CameraHold {
    /// Aucun ordre donné depuis le lancement.
    #[default]
    Unset,
    /// Le montage est libre.
    Free,
    /// Ce plan est tenu.
    On(&'static str),
}

/// Identifiant du plan d'attente dans le catalogue du réalisateur.
const WAITING_SHOT: &str = "waiting";
/// Identifiant du plan-séquence de l'ouverture.
const OPENING_SHOT: &str = "opening";
/// Identifiant du plan-séquence de la clôture.
const CLOSING_SHOT: &str = "closing";
/// Identifiant du plan de passation de DJ.
const HANDOVER_SHOT: &str = "handover";

/// Le décor est au repos : pas en `Running` et aucune tour en place. Pur, pour être testé.
#[must_use]
fn scene_is_idle(phase: Phase, build: f32) -> bool {
    phase != Phase::Running && build <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::timeline::{RunStages, starting_stages, stopping_stages};

    #[test]
    fn the_scene_is_idle_while_waiting_and_never_while_running() {
        assert!(scene_is_idle(Phase::Waiting, RunStages::WAITING.build));
        assert!(!scene_is_idle(Phase::Running, 0.0));
        assert!(!scene_is_idle(Phase::Running, 1.0));
    }

    #[test]
    fn it_wakes_with_the_first_tower_at_the_start_and_sleeps_with_the_last_one_at_the_stop() {
        assert!(scene_is_idle(Phase::Starting, starting_stages(0.0, false).build));
        assert!(
            scene_is_idle(Phase::Starting, starting_stages(12.0, false).build),
            "route nue avant le portique"
        );
        assert!(!scene_is_idle(Phase::Starting, starting_stages(22.0, false).build));
        assert!(!scene_is_idle(Phase::Stopping, stopping_stages(3.0).build));
        assert!(scene_is_idle(Phase::Stopping, stopping_stages(13.0).build));
    }
}
