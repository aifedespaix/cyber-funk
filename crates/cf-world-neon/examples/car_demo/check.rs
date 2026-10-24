//! Le mode `CF_CHECK=1` de `car_demo` : scènes scriptées, captures, verdicts sur les pixels.
//!
//! L'horloge avance d'exactement 1/60 s par image (`TimeUpdateStrategy::ManualDuration`, posé par `main`) et le temps du
//! shader de la route est imposé (`RoadLook::clock_s`) : la capture d'une scène ne dépend ni de la vitesse de la
//! machine, ni du moment où elle tombe. Le point suivi est immobile, sauf dans les scènes de conduite où le script le
//! déplace image par image (la voiture mesure son mouvement sur sa position : c'est ce qu'elle fera avec le pilote).

use std::collections::HashMap;
use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::{AbsolutePosition, AudioFeatures};
use cf_world_neon::RoadLook;
use cf_world_neon::car::{Car, CarControls, CarState};

use crate::shots::{Rect, Shot, luma};
use crate::{SIZE, View};

/// Temps réel (s) laissé au démarrage : décodage du modèle et compilation des pipelines (rendu logiciel).
const WARMUP_S: f64 = 25.0;
/// Images laissées à une scène pour que l'origine, les transforms et les matériaux soient à jour avant la capture.
const SETTLE_FRAMES: u32 = 12;
/// Profondeur du point de départ, m.
const START_Z: f64 = -1234.5;

/// Une scène : ce que la musique et le pilote font, la vue, le mouvement scripté.
#[derive(Clone, Copy)]
struct Scene {
    name: &'static str,
    view: View,
    pulse: f32,
    bass: f32,
    steer: f32,
    brake: f32,
    /// Images de settle propres à la scène (le clignotant ne dure que 0,33 s après le volant).
    settle: u32,
    /// Conduite scriptée : (vitesse m/s, amplitude de la vitesse latérale m/s) appliquées à la position à chaque image ;
    /// la vitesse latérale est un cosinus de 0,6 Hz (un slalom de ±3,7 m : la voiture reste sur la route).
    drive: Option<(f64, f64)>,
}

const BASE: Scene = Scene {
    name: "",
    view: View::Chase,
    pulse: 0.0,
    bass: 0.0,
    steer: 0.0,
    brake: 0.0,
    settle: SETTLE_FRAMES,
    drive: None,
};

const SCENES: [Scene; 9] = [
    Scene {
        name: "1-roulage",
        ..BASE
    },
    Scene {
        name: "2-kick",
        pulse: 1.0,
        bass: 0.8,
        ..BASE
    },
    Scene {
        name: "3-freinage",
        brake: 1.0,
        ..BASE
    },
    Scene {
        name: "4-clignotant",
        steer: 0.8,
        settle: 3,
        ..BASE
    },
    Scene {
        name: "5-glissade",
        drive: Some((40.0, 14.0)),
        settle: 50,
        ..BASE
    },
    Scene {
        name: "6-avant-calme",
        view: View::FrontThreeQuarter,
        ..BASE
    },
    Scene {
        name: "7-avant-kick",
        view: View::FrontThreeQuarter,
        pulse: 1.0,
        bass: 0.8,
        ..BASE
    },
    Scene {
        name: "8-arriere-calme",
        view: View::RearThreeQuarter,
        ..BASE
    },
    Scene {
        name: "9-arriere-freinage",
        view: View::RearThreeQuarter,
        brake: 1.0,
        ..BASE
    },
];

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Enter,
    Settle(u32),
    Await,
}

#[derive(Resource, Default)]
pub(crate) struct Check {
    start: Option<Instant>,
    index: usize,
    stage: Stage,
    shots: HashMap<&'static str, Shot>,
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut audio: ResMut<AudioFeatures>,
    mut look: ResMut<RoadLook>,
    mut controls: ResMut<CarControls>,
    mut view: ResMut<View>,
    mut car: Query<(&mut AbsolutePosition, &mut CarState), With<Car>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    let Ok((mut position, mut state)) = car.single_mut() else {
        // La voiture n'est pas encore posée : le modèle se décode (le temps de chargement ne compte pas).
        check.start = Some(Instant::now());
        return;
    };
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    let Some(scene) = SCENES.get(check.index).copied() else {
        exit.write(evaluate(&check));
        return;
    };
    match check.stage {
        Stage::Enter => {
            position.0 = DVec3::new(0.0, 0.0, START_Z);
            // Une téléportation n'est pas un déplacement : la voiture repart d'une mesure vierge.
            state.previous = None;
            state.warm = false;
            state.speed_mps = 0.0;
            state.lateral_mps = 0.0;
            state.accel_mps2 = 0.0;
            audio.0.live = true;
            audio.0.pulse = scene.pulse;
            audio.0.bass = scene.bass;
            audio.0.intensity = 0.7;
            controls.steer = scene.steer;
            controls.brake = scene.brake;
            *view = scene.view;
            look.clock_s = Some(4.0);
            check.stage = Stage::Settle(0);
        }
        Stage::Settle(frames) if frames < scene.settle => {
            if let Some((speed, amplitude)) = scene.drive {
                let lateral = amplitude * (std::f64::consts::TAU * 0.6 * f64::from(frames) / 60.0).cos();
                position.0 += DVec3::new(lateral / 60.0, 0.0, -speed / 60.0);
            }
            check.stage = Stage::Settle(frames + 1);
        }
        Stage::Settle(_) => {
            let name = scene.name;
            let mut shot = commands.spawn(Screenshot::primary_window());
            shot.observe(move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>| {
                if let Ok(image) = captured.image.clone().try_into_dynamic() {
                    let rgba = image.to_rgba8();
                    let width = rgba.width();
                    check.shots.insert(
                        name,
                        Shot {
                            width,
                            rgba: rgba.into_raw(),
                        },
                    );
                }
                check.index += 1;
                check.stage = Stage::Enter;
            });
            if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
                let dir = std::path::PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
            }
            check.stage = Stage::Await;
        }
        Stage::Await => {}
    }
}

/// Le centre de l'image, là où la poursuite montre la voiture.
const CAR_CHASE: Rect = ((280, 250), (680, 540));
/// Toute l'image sauf le bord : les vues trois quarts cadrent la voiture au centre.
const CAR_CLOSE: Rect = ((160, 90), (800, 450));

fn evaluate(check: &Check) -> AppExit {
    let mut ok = true;
    let mut verdict = |pass: bool, what: String| {
        if pass {
            info!("OK     {what}");
        } else {
            error!("ÉCHEC  {what}");
            ok = false;
        }
    };
    let shot = |name: &str| check.shots.get(name);
    let (Some(roll), Some(kick), Some(brake), Some(blink)) = (
        shot("1-roulage"),
        shot("2-kick"),
        shot("3-freinage"),
        shot("4-clignotant"),
    ) else {
        error!("captures de la poursuite absentes");
        return AppExit::error();
    };
    verdict(
        roll.width == SIZE.0 && roll.height() == SIZE.1,
        format!("capture de {}×{} px", roll.width, roll.height()),
    );
    let bright = |p: [f32; 3]| luma(p) > 90.0;
    let seen = roll.count_in(CAR_CHASE, bright);
    verdict(
        seen > 300,
        format!("la voiture est dessinée ({seen} pixels lumineux au centre)"),
    );
    // Un feu stop plein : rouge franc (corail / rouge d'alarme), peu de vert et de bleu.
    let red = |p: [f32; 3]| p[0] > 170.0 && p[1] < 110.0 && p[2] < 110.0;
    let (rolling_red, braking_red) = (roll.count_in(CAR_CHASE, red), brake.count_in(CAR_CHASE, red));
    verdict(
        braking_red > rolling_red + 40,
        format!("freinage : les feux stop s'allument ({rolling_red} → {braking_red} pixels rouges)"),
    );
    // Clignotant : orange (ambre).
    let amber = |p: [f32; 3]| p[0] > 200.0 && p[1] > 90.0 && p[1] < 190.0 && p[2] < 90.0;
    let (calm_amber, blink_amber) = (roll.count_in(CAR_CHASE, amber), blink.count_in(CAR_CHASE, amber));
    verdict(
        blink_amber > calm_amber + 15,
        format!("clignotant : l'ambre apparaît ({calm_amber} → {blink_amber} pixels orange)"),
    );
    let (calm_luma, kick_luma) = (roll.mean_luma(CAR_CHASE), kick.mean_luma(CAR_CHASE));
    verdict(
        kick_luma > calm_luma + 0.5,
        format!("le kick fait pulser la voiture et la route ({calm_luma:.2} → {kick_luma:.2} de luminance moyenne)"),
    );
    if let (Some(calm), Some(pulse)) = (shot("6-avant-calme"), shot("7-avant-kick")) {
        let (a, b) = (calm.mean_luma(CAR_CLOSE), pulse.mean_luma(CAR_CLOSE));
        verdict(
            b > a + 0.5,
            format!("vue trois quarts avant : le kick éclaire la voiture ({a:.2} → {b:.2})"),
        );
        let lit = calm.count_in(CAR_CLOSE, bright);
        verdict(
            lit > 600,
            format!("vue trois quarts avant : la voiture se voit ({lit} pixels lumineux)"),
        );
    }
    if let (Some(calm), Some(braking)) = (shot("8-arriere-calme"), shot("9-arriere-freinage")) {
        let (a, b) = (calm.count_in(CAR_CLOSE, red), braking.count_in(CAR_CLOSE, red));
        verdict(
            b > a + 100,
            format!("vue trois quarts arrière : feux stop et flaque rouge ({a} → {b} pixels rouges)"),
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
