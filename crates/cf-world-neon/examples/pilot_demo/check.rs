//! Le mode `CF_CHECK=1` de `pilot_demo` : avance le pilote en accéléré jusqu'à ce qu'il vole, capture, vérifie.
//!
//! L'horloge avance d'exactement 1/60 s par image (`TimeUpdateStrategy::ManualDuration`, posé par `main`). Pour ne pas
//! attendre des minutes de rendu logiciel, [`fast_forward`] joue des pas de pilote supplémentaires (la logique seule)
//! tant qu'on cherche un envol ou une figure ; dès qu'on les tient, il s'arrête et la scène continue en temps normal
//! le temps de la capture.

use std::time::Instant;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::AudioFeatures;
use cf_world_neon::car::CarRig;
use cf_world_neon::pilot::{BeatInput, PilotBrain, PilotSettings, PilotTick, PilotTraffic};
use cf_world_neon::{CarAnimation, RoadLook};

use crate::SIZE;

/// Temps réel (s) laissé au démarrage : décodage du modèle et compilation des pipelines (rendu logiciel).
const WARMUP_S: f64 = 25.0;
/// Pas de pilote joués en plus à chaque image pendant la recherche : ~12 s de conduite par seconde de rendu à 1 i/s.
const STEPS_PER_FRAME: u32 = 24;
/// Images laissées à la scène pour que l'origine, les transforms et les matériaux soient à jour avant la capture.
const SETTLE_FRAMES: u32 = 12;
/// Niveau 0 : durée de conduite vérifiée, en pas de pilote à 60 i/s (une minute et demie ; les tests de simulation en
/// couvrent dix, ici il s'agit de montrer la voiture au sol).
const LEVEL_0_STEPS: u32 = 60 * 90;

/// Niveau du spectacle imposé au pilote.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct Level(pub(crate) u8);

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Stage {
    /// Le modèle se décode, les pipelines se compilent.
    #[default]
    Warm,
    /// Accéléré, jusqu'à une altitude d'étage.
    HuntFlight,
    /// En vol : quelques images pour que tout soit à jour, puis capture.
    Settle(u32),
    /// Accéléré, jusqu'au départ d'une figure.
    HuntTrick,
    /// La figure se déroule en temps normal ; capture à mi-course.
    WaitTrick(u32),
    /// Une capture est demandée, on attend qu'elle arrive.
    Await,
    Done,
}

#[derive(Resource, Default)]
pub(crate) struct Check {
    start: Option<Instant>,
    stage: Stage,
    /// Le pilote a-t-il quitté le sol à un moment (suivi image par image, accéléré compris) ?
    ever_airborne: bool,
    max_altitude: f32,
    /// Pas de pilote joués (accélérés compris).
    steps: u32,
    shots: Vec<(&'static str, f32)>,
    pending: u32,
}

/// `CfSet::Simulate`, avant le pilote : des pas de plus tant qu'on cherche (voir le doc du module).
pub(crate) fn fast_forward(
    mut check: ResMut<Check>,
    level: Res<Level>,
    features: Res<AudioFeatures>,
    settings: Res<PilotSettings>,
    traffic: Res<PilotTraffic>,
    mut brain: ResMut<PilotBrain>,
    rigs: Query<(), With<CarRig>>,
) {
    let hunting = if level.0 == 0 {
        check.steps < LEVEL_0_STEPS
    } else {
        matches!(check.stage, Stage::HuntFlight | Stage::HuntTrick)
    };
    if rigs.is_empty() || !hunting || check.stage == Stage::Warm {
        return;
    }
    let mut beat = BeatInput::from(&features.0);
    beat.kick = false;
    for _ in 0..STEPS_PER_FRAME {
        let out = brain.0.update(&PilotTick {
            dt: 1.0 / 60.0,
            beat,
            show: settings.show,
            settings: &settings,
            traffic: &traffic,
            overtakes: 0,
        });
        // Le suivi couvre chaque pas accéléré : « jamais quitté le sol » vaut pour toute la conduite, pas pour les
        // seules images rendues.
        check.ever_airborne |= out.animation.airborne();
        check.max_altitude = check.max_altitude.max(out.animation.altitude);
        check.steps += 1;
    }
}

fn screenshot(commands: &mut Commands, name: &'static str) {
    let mut shot = commands.spawn(Screenshot::primary_window());
    shot.observe(move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>| {
        let dark = captured.image.clone().try_into_dynamic().map_or(0.0, |image| {
            // Pixels lumineux : la voiture, ses néons, la route éclairée.
            let rgba = image.to_rgba8();
            let bright = rgba
                .pixels()
                .filter(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) > 90.0);
            bright.count() as f32
        });
        check.shots.push((name, dark));
        check.pending = check.pending.saturating_sub(1);
    });
    if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
    }
}

/// `CfSet::Present` : la machine à états de la vérification.
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    level: Res<Level>,
    animation: Res<CarAnimation>,
    mut look: ResMut<RoadLook>,
    rigs: Query<(), With<CarRig>>,
    mut exit: MessageWriter<AppExit>,
) {
    if rigs.is_empty() {
        // La voiture n'est pas encore posée : le temps de chargement ne compte pas.
        check.start = Some(Instant::now());
        return;
    }
    let start = *check.start.get_or_insert_with(Instant::now);
    look.clock_s = Some(4.0);
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    check.ever_airborne |= animation.airborne();
    check.max_altitude = check.max_altitude.max(animation.altitude);

    if level.0 == 0 {
        match check.stage {
            Stage::Warm => check.stage = Stage::HuntFlight,
            Stage::HuntFlight if check.steps >= LEVEL_0_STEPS => {
                check.pending = 1;
                screenshot(&mut commands, "niveau-0-au-sol");
                check.stage = Stage::Await;
            }
            Stage::Await if check.pending == 0 => {
                check.stage = Stage::Done;
                exit.write(evaluate(&check, level.0));
            }
            _ => {}
        }
        return;
    }

    match check.stage {
        Stage::Warm => check.stage = Stage::HuntFlight,
        Stage::HuntFlight if animation.floor >= 3 && animation.altitude > 12.0 => check.stage = Stage::Settle(0),
        Stage::Settle(n) if n < SETTLE_FRAMES => check.stage = Stage::Settle(n + 1),
        Stage::Settle(_) => {
            check.pending += 1;
            screenshot(&mut commands, "niveau-5-en-vol");
            check.stage = Stage::HuntTrick;
        }
        Stage::HuntTrick if animation.trick.is_some() => check.stage = Stage::WaitTrick(0),
        Stage::WaitTrick(n) => {
            if animation.trick_roll.abs() + animation.trick_pitch.abs() > 1.5 {
                check.pending += 1;
                screenshot(&mut commands, "niveau-5-figure");
                check.stage = Stage::Await;
            } else if n > 120 || animation.trick.is_none() {
                // La figure s'est terminée sans passer le seuil (ne devrait pas arriver) : on en cherche une autre.
                check.stage = Stage::HuntTrick;
            } else {
                check.stage = Stage::WaitTrick(n + 1);
            }
        }
        Stage::Await if check.pending == 0 => {
            check.stage = Stage::Done;
            exit.write(evaluate(&check, level.0));
        }
        _ => {}
    }
}

fn evaluate(check: &Check, level: u8) -> AppExit {
    let mut ok = true;
    let mut verdict = |pass: bool, what: String| {
        if pass {
            info!("OK     {what}");
        } else {
            error!("ÉCHEC  {what}");
            ok = false;
        }
    };
    verdict(
        !check.shots.is_empty(),
        format!("captures de {}×{} px : {} reçue(s)", SIZE.0, SIZE.1, check.shots.len()),
    );
    for (name, bright) in &check.shots {
        verdict(
            *bright > 3000.0,
            format!("{name} : la scène est dessinée ({bright} pixels lumineux)"),
        );
    }
    if level == 0 {
        verdict(
            !check.ever_airborne && check.max_altitude < 0.001,
            format!(
                "niveau 0 : la voiture n'a jamais quitté le sol en {} pas de pilote (altitude max {} m)",
                check.steps, check.max_altitude
            ),
        );
    } else {
        verdict(
            check.ever_airborne && check.max_altitude > 12.0,
            format!(
                "niveau {level} : la voiture vole (altitude max {} m)",
                check.max_altitude
            ),
        );
        verdict(
            check.shots.len() == 2,
            "une capture en vol et une capture en pleine figure".into(),
        );
    }
    if ok {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        error!("vérification : échec");
        AppExit::error()
    }
}
