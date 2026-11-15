//! Mode `--rehearse <dossier>` : la répétition filmée. L'application joue seule la séquence complète (attente,
//! `Démarrer`, quelques secondes de show, `Arrêter`, retour à l'attente) à pas de temps fixe, écrit une capture du
//! projecteur à intervalle régulier dans le dossier, puis se ferme.
//!
//! C'est l'outil pour juger l'ouverture et la fin à l'image (mise en scène, raccords de caméra) sur une machine sans
//! écran : sous `Xvfb` et rendu logiciel la cadence réelle est de quelques images par seconde, d'où le pas fixe, qui
//! donne la même séquence quelle que soit la vitesse de la machine.
//!
//! Réglages par variables d'environnement (outil de développement, pas une interface d'opérateur) :
//! `CF_REHEARSE_RUN_S` (secondes de show entre la fin du démarrage et `Arrêter`, 12 par défaut),
//! `CF_REHEARSE_EVERY_S` (période des captures, 0,5 s), `CF_REHEARSE_STEP_S` (pas de temps d'une image, 0,05 s).

use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use cf_render::{AppState, Phase, RunOrder, RunPhase};

/// Attente filmée avant `Démarrer` (s).
const LEAD_IN_S: f32 = 9.0;
/// Attente filmée après la fin de l'arrêt (s).
const LEAD_OUT_S: f32 = 4.0;

/// Branche la répétition filmée.
#[derive(Debug, Clone)]
pub struct RehearsePlugin {
    /// Dossier où écrire les captures.
    pub dir: PathBuf,
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(default)
}

impl Plugin for RehearsePlugin {
    fn build(&self, app: &mut App) {
        if let Err(error) = std::fs::create_dir_all(&self.dir) {
            tracing::error!(%error, dir = %self.dir.display(), "répétition filmée désactivée");
            return;
        }
        let step_s = env_f32("CF_REHEARSE_STEP_S", 0.05);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(step_s)))
            .insert_resource(Rehearsal {
                dir: self.dir.clone(),
                run_s: env_f32("CF_REHEARSE_RUN_S", 12.0),
                every_s: env_f32("CF_REHEARSE_EVERY_S", 0.5),
                step: Step::LeadIn,
                in_step_s: 0.0,
                clock_s: 0.0,
                next_shot_s: 0.0,
                shots: 0,
            })
            .add_systems(Update, rehearse.run_if(in_state(AppState::Ready)));
    }
}

/// Où en est la répétition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    LeadIn,
    Starting,
    Running,
    Stopping,
    LeadOut,
}

#[derive(Resource, Debug)]
struct Rehearsal {
    dir: PathBuf,
    run_s: f32,
    every_s: f32,
    step: Step,
    in_step_s: f32,
    clock_s: f32,
    next_shot_s: f32,
    shots: u32,
}

/// Déroule la répétition : envoie les ordres, écrit les captures, ferme l'application à la fin.
fn rehearse(
    time: Res<Time>,
    phase: Res<RunPhase>,
    mut state: ResMut<Rehearsal>,
    mut orders: MessageWriter<RunOrder>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    state.clock_s += dt;
    state.in_step_s += dt;
    let before = state.step;
    match (state.step, phase.phase) {
        (Step::LeadIn, Phase::Waiting) if state.in_step_s >= LEAD_IN_S => {
            // Renvoyé tant que le séquenceur ne l'a pas pris (il ignore les ordres hors de l'attente).
            orders.write(RunOrder::Start);
        }
        (Step::LeadIn, Phase::Starting) => state.step = Step::Starting,
        (Step::Starting, Phase::Running) => state.step = Step::Running,
        (Step::Running, Phase::Running) if state.in_step_s >= state.run_s => {
            orders.write(RunOrder::Stop);
        }
        (Step::Running, Phase::Stopping) => state.step = Step::Stopping,
        (Step::Stopping, Phase::Waiting) => state.step = Step::LeadOut,
        (Step::LeadOut, _) if state.in_step_s >= LEAD_OUT_S => {
            tracing::info!(shots = state.shots, "répétition filmée terminée");
            exit.write(AppExit::Success);
        }
        _ => {}
    }
    if state.step != before {
        state.in_step_s = 0.0;
    }
    if state.clock_s >= state.next_shot_s {
        state.next_shot_s += state.every_s;
        let label = match state.step {
            Step::LeadIn => "attente",
            Step::Starting => "demarrage",
            Step::Running => "show",
            Step::Stopping => "arret",
            Step::LeadOut => "fin",
        };
        let path = state
            .dir
            .join(format!("{:04}-{label}-{:05.1}.png", state.shots, state.in_step_s));
        state.shots += 1;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}
