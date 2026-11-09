//! Applique la séquence au monde : le plugin et le système `drive_run`.
//!
//! ```text
//! RunOrder ─▶ RunSequencer ─▶ RunOutput ─┬▶ RunPhase / RunStatus  (l'opérateur et les autres systèmes du monde le lisent)
//!                                        ├▶ RoadLook.draw, FacadeLook.build, SkyLook.reveal, DjSignIntro
//!                                        ├▶ CarControls.light_scale, PilotSettings.speed_cap_mps
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
    RunPhase, SceneIdle, ShowSheet, SimControl,
};

use super::machine::{RunOutput, RunSequencer};
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
    mut silenced: Local<Option<bool>>,
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
    let wanted = if out.stages.camera_handover {
        Some(HANDOVER_SHOT)
    } else if out.stages.camera_waiting {
        Some(WAITING_SHOT)
    } else {
        None
    };
    direct_camera(wanted, &mut camera_held, &mut shots);
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
    gate_spontaneous(&mut targets, out.phase, &mut silenced);
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
        // Le nom du DJ est annoncé par le portique (`gantry/`), plus par l'enseigne sur immeuble de la pré-intro.
        set(&mut d.0, None);
    }
    if let Some(c) = targets.controls.as_deref_mut() {
        set(&mut c.light_scale, s.lights);
        set(&mut c.presence, s.car_presence);
        set(&mut c.calm, calm);
    }
    if let Some(p) = targets.pilot.as_deref_mut() {
        set(&mut p.speed_cap_mps, out.speed_cap_mps.map(f64::from));
    }
    if let Some(t) = targets.traffic.as_deref_mut() {
        intro::apply_front(&mut t.scene.fx.power, out.front, owned);
    }
}

/// En attente et pendant la construction, la circulation ne tire rien ; on la rend telle qu'on l'a trouvée ensuite
/// (le mode manuel de l'opérateur l'avait peut-être déjà coupée).
fn gate_spontaneous(targets: &mut Targets, phase: Phase, silenced: &mut Option<bool>) {
    let Some(t) = targets.traffic.as_deref_mut() else {
        return;
    };
    let quiet = matches!(phase, Phase::Waiting | Phase::Starting | Phase::Handover);
    match (quiet, *silenced) {
        (true, None) => {
            *silenced = Some(t.spontaneous());
            t.set_spontaneous(false);
        }
        (false, Some(previous)) => {
            t.set_spontaneous(previous);
            *silenced = None;
        }
        _ => {}
    }
}

/// Le plan voulu tient la caméra : l'attente avant la voiture, la passation entre l'échange et le dépassement. Sans plan
/// voulu, le montage reprend. Au retour à l'attente (arrêt), le plan revient en fondu et non en coupe.
fn direct_camera(wanted: Option<&'static str>, held: &mut CameraHold, shots: &mut MessageWriter<ShotOrder>) {
    match (wanted, *held) {
        // Le retour à l'attente, après un autre plan, se fait en fondu.
        (Some(id), CameraHold::Free) if id == WAITING_SHOT => {
            shots.write(ShotOrder::HoldWith(id, TransitionKind::Blend));
            *held = CameraHold::On(id);
        }
        (Some(id), CameraHold::Unset | CameraHold::Free) => {
            shots.write(ShotOrder::Hold(id));
            *held = CameraHold::On(id);
        }
        (Some(id), CameraHold::On(current)) if current != id => {
            shots.write(ShotOrder::Hold(id));
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
            scene_is_idle(Phase::Starting, starting_stages(15.0, false).build),
            "route nue sous le portique"
        );
        assert!(!scene_is_idle(Phase::Starting, starting_stages(22.0, false).build));
        assert!(!scene_is_idle(Phase::Stopping, stopping_stages(3.0).build));
        assert!(scene_is_idle(Phase::Stopping, stopping_stages(7.0).build));
    }
}
