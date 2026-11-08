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

use bevy::prelude::*;
use cf_render::{AudioFeatures, CfSet, Phase, RunOrder, RunPhase, SimControl};

use super::machine::{RunOutput, RunSequencer};
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
            .insert_resource(RunDriven)
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

/// `CfSet::Simulate` : lit l'ordre de l'opérateur, avance la séquence et l'applique au monde.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
fn drive_run(
    time: Res<Time>,
    mut orders: MessageReader<RunOrder>,
    features: Option<Res<AudioFeatures>>,
    sim: Option<ResMut<SimControl>>,
    mut sequencer: ResMut<Sequencer>,
    mut phase: ResMut<RunPhase>,
    mut status: ResMut<RunStatus>,
    mut targets: Targets,
    mut owned: Local<Owned>,
    mut silenced: Local<Option<bool>>,
) {
    let order = orders.read().last().copied();
    let before = sequencer.0.phase();
    let kick = features.is_some_and(|f| f.0.kick);
    let out = sequencer.0.update(order, time.delta_secs(), kick);
    // Chaque `Démarrer` repart d'un show neuf : le chef du spectacle, l'horloge et la piste de démo sont remis à zéro.
    if before == Phase::Waiting && out.phase == Phase::Starting {
        if let Some(mut sim) = sim {
            sim.request_reset();
        }
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
    set(&mut *phase, published);
    set(&mut status.0, out);

    // Hors séquence en cours, le séquenceur n'a la main qu'en attente ; en `Running` il rend tout une fois.
    let in_charge = out.phase != Phase::Running;
    if !in_charge && !owned.engaged {
        return;
    }
    owned.engaged = in_charge;
    apply(&out, &mut targets, &mut owned);
    gate_spontaneous(&mut targets, out.phase, &mut silenced);
}

/// Pose les étapes sur les cibles ; une cible absente est ignorée (exemples, tests).
fn apply(out: &RunOutput, targets: &mut Targets, owned: &mut Owned) {
    let s = out.stages;
    let driving = out.phase != Phase::Running;
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
        set(&mut d.0, driving.then_some(s.sign));
    }
    if let Some(c) = targets.controls.as_deref_mut() {
        set(&mut c.light_scale, s.lights);
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
    let quiet = matches!(phase, Phase::Waiting | Phase::Starting);
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
