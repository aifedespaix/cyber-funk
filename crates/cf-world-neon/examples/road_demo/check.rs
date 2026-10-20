//! Le mode `CF_CHECK=1` de `road_demo` : scènes scriptées, captures, réglages envoyés, verdicts sur les pixels.
//!
//! L'horloge avance d'exactement 1/60 s par image (`TimeUpdateStrategy::ManualDuration`, posé par `main`) et le temps du
//! shader est imposé (`RoadLook::clock_s`) : la capture d'une scène ne dépend ni de la vitesse de la machine, ni du
//! moment où elle tombe. Le point suivi est immobile : c'est le script qui le téléporte.

use std::collections::HashMap;
use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::{AbsolutePosition, AudioFeatures, CameraPose, FloatingOrigin};
use cf_world_neon::{RoadLook, Runner};

use crate::SIZE;
use crate::json::{Row, scenes_json};
use crate::shots::{Shot, differing_share, luma, worst_diff};

/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 8.0;
/// Images laissées à une scène pour que l'origine, les transforms et le matériau soient à jour avant la capture.
const SETTLE_FRAMES: u32 = 8;

/// Une scène : où est le point suivi, ce que la musique et la météo font.
#[derive(Clone, Copy)]
struct Scene {
    name: &'static str,
    /// Passer d'abord par cette profondeur (m) : l'origine flottante y reste ensuite par hystérésis.
    via_z: Option<f64>,
    car_z: f64,
    time_s: f64,
    pulse: f32,
    bass: f32,
    intensity: f32,
    wet: f32,
    rain: f32,
    draw: f32,
    brake: f32,
    /// Le pavé du point suivi est affiché (sinon la route seule, comparable à Sillage).
    car: bool,
}

const BASE: Scene = Scene {
    name: "",
    via_z: None,
    car_z: 0.0,
    time_s: 4.0,
    pulse: 0.4,
    bass: 0.3,
    intensity: 0.7,
    wet: 0.0,
    rain: 0.0,
    draw: 1.0,
    brake: 0.0,
    car: false,
};

/// Recentrage d'origine : 28 800 m, 14 pas = 403,2 km.
const KM_403: f64 = -403_200.0;

const SCENES: [Scene; 10] = [
    Scene {
        name: "1-depart",
        car_z: -1234.5,
        time_s: 3.0,
        pulse: 0.0,
        bass: 0.0,
        intensity: 0.4,
        ..BASE
    },
    Scene {
        name: "2-kick",
        car_z: -1234.5,
        time_s: 7.25,
        pulse: 1.0,
        bass: 0.8,
        intensity: 0.9,
        ..BASE
    },
    Scene {
        name: "3-pluie",
        car_z: -5000.0,
        time_s: 12.0,
        pulse: 0.3,
        bass: 0.2,
        intensity: 0.5,
        wet: 1.0,
        rain: 0.8,
        ..BASE
    },
    // Même point de vue que le départ : seule la part dessinée change (110 m sur 1 100).
    Scene {
        name: "4-pre-intro",
        car_z: -1234.5,
        time_s: 3.0,
        pulse: 0.0,
        bass: 0.0,
        intensity: 0.4,
        draw: 0.1,
        ..BASE
    },
    Scene {
        name: "5-freinage-mouille",
        car_z: -777.0,
        time_s: 9.0,
        pulse: 0.5,
        wet: 1.0,
        brake: 1.0,
        ..BASE
    },
    Scene {
        name: "6-poursuite",
        car_z: -3000.0,
        time_s: 6.0,
        car: true,
        ..BASE
    },
    // Le même point de vue à 403,2 km d'écart (un multiple du pas d'origine, donc des périodes des motifs) : la route doit
    // être aussi nette. Les teintes, elles, suivent la coordonnée absolue et diffèrent d'un point de vue à l'autre.
    Scene {
        name: "7-km0",
        car_z: 0.0,
        ..BASE
    },
    Scene {
        name: "8-km403",
        car_z: KM_403,
        ..BASE
    },
    // Et de part et d'autre d'un recentrage : même point de vue (−14 000 m), mais l'origine est 0 dans un cas et
    // −28 800 dans l'autre (on y arrive de loin : l'hystérésis la garde).
    Scene {
        name: "9-seuil-direct",
        car_z: -14_000.0,
        ..BASE
    },
    Scene {
        name: "10-seuil-via-km30",
        via_z: Some(-30_000.0),
        car_z: -14_000.0,
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
    via_done: bool,
    shots: HashMap<&'static str, Shot>,
    rows: Vec<Row>,
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut audio: ResMut<AudioFeatures>,
    mut look: ResMut<RoadLook>,
    origin: Res<FloatingOrigin>,
    pose: Res<CameraPose>,
    mut runner: Query<(&mut AbsolutePosition, &mut Visibility), With<Runner>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    let Some(scene) = SCENES.get(check.index).copied() else {
        exit.write(evaluate(&check));
        return;
    };
    let Ok((mut car, mut visibility)) = runner.single_mut() else {
        return;
    };
    match check.stage {
        Stage::Enter => {
            // Étape intermédiaire : on va d'abord loin, pour que l'origine flottante s'y recentre.
            let z = match scene.via_z {
                Some(via) if !check.via_done => {
                    check.via_done = true;
                    via
                }
                _ => scene.car_z,
            };
            car.0 = DVec3::new(0.0, 0.65, z);
            *visibility = if scene.car {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            audio.0.pulse = scene.pulse;
            audio.0.bass = scene.bass;
            audio.0.intensity = scene.intensity;
            look.wet = scene.wet;
            look.rain = scene.rain;
            look.draw = scene.draw;
            look.brake = scene.brake;
            look.clock_s = Some(scene.time_s);
            check.stage = Stage::Settle(0);
        }
        Stage::Settle(frames) if frames < SETTLE_FRAMES => check.stage = Stage::Settle(frames + 1),
        Stage::Settle(_) => {
            if scene.via_z.is_some() && check.via_done && car.0.z.to_bits() != scene.car_z.to_bits() {
                // Premier passage (loin) terminé : on revient au point de vue de la scène.
                car.0.z = scene.car_z;
                check.stage = Stage::Settle(0);
                return;
            }
            check.rows.push(Row {
                name: scene.name,
                car_z: scene.car_z,
                time_s: scene.time_s,
                pulse: scene.pulse,
                bass: scene.bass,
                intensity: scene.intensity,
                wet: scene.wet,
                rain: scene.rain,
                draw: scene.draw,
                brake: scene.brake,
                car: scene.car,
                origin_z: origin.origin().z,
                fov_deg: pose.fov_deg,
                camera: pose.position.to_array(),
                forward: (pose.rotation * Vec3::NEG_Z).to_array(),
            });
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
                check.via_done = false;
                check.stage = Stage::Enter;
            });
            if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
                let dir = std::path::PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
                let _ = std::fs::write(dir.join("scenes.json"), scenes_json(&check.rows));
            }
            check.stage = Stage::Await;
        }
        Stage::Await => {}
    }
}

/// Route proche de la caméra : le bas de l'écran, au centre.
const NEAR_ROAD: ((u32, u32), (u32, u32)) = ((300, 400), (660, 530));
/// L'horizon de la route : là où la pré-intro dessine son front lumineux (à ~110 m avec `draw = 0,1`).
const FRONT: ((u32, u32), (u32, u32)) = ((430, 240), (530, 252));

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
    let (Some(depart), Some(kick)) = (shot("1-depart"), shot("2-kick")) else {
        error!("captures de départ absentes");
        return AppExit::error();
    };
    verdict(
        depart.width == SIZE.0 && depart.height() == SIZE.1,
        format!("capture de {}×{} px", depart.width, depart.height()),
    );
    let lit = depart.count(|p| luma(p) > 80.0);
    verdict(lit > 400, format!("la route est dessinée ({lit} pixels lumineux)"));
    let (calm, pulse) = (
        depart.mean_luma(NEAR_ROAD.0, NEAR_ROAD.1),
        kick.mean_luma(NEAR_ROAD.0, NEAR_ROAD.1),
    );
    verdict(
        pulse > calm + 1.0,
        format!("le kick fait pulser la route ({calm:.1} → {pulse:.1})"),
    );
    if let Some(intro) = shot("4-pre-intro") {
        let bright = |p: [f32; 3]| luma(p) > 60.0;
        let (full, front) = (depart.count_in(FRONT, bright), intro.count_in(FRONT, bright));
        verdict(
            front > full + 30,
            format!("pré-intro : un front lumineux se dessine à l'horizon ({full} → {front} pixels vifs)"),
        );
    }
    if let Some(rain) = shot("3-pluie") {
        let share = differing_share(depart, rain, 8.0);
        verdict(
            share > 0.05,
            format!("pluie : la chaussée change ({:.0} % des pixels)", share * 100.0),
        );
    }
    if let Some(drive) = shot("6-poursuite") {
        // Le pavé du point suivi : émissif (0,4 ; 0,15 ; 1,2) linéaire, soit environ (171, 110, 255) en sRGB.
        let n = drive.count(|p| (p[0] - 171.0).abs() < 12.0 && (p[1] - 110.0).abs() < 12.0 && p[2] > 235.0);
        verdict(
            n > 300,
            format!("poursuite : la caméra voit le point suivi ({n} pixels)"),
        );
    }
    let origins = |a: &str, b: &str| {
        let z = |name: &str| check.rows.iter().find(|r| r.name == name).map(|r| r.origin_z);
        z(a).zip(z(b))
    };
    if let Some((near, far)) = shot("7-km0").zip(shot("8-km403")) {
        let (a, b) = (near.count(|p| luma(p) > 80.0), far.count(|p| luma(p) > 80.0));
        verdict(
            a.abs_diff(b) * 20 < a,
            format!("km 403 : la route est aussi nette qu'au départ ({a} pixels lumineux au km 0, {b} au km 403)"),
        );
    }
    for (a, b) in [("9-seuil-direct", "10-seuil-via-km30")] {
        let (Some(x), Some(y)) = (shot(a), shot(b)) else {
            verdict(false, format!("{a} / {b} : captures absentes"));
            continue;
        };
        let (worst, share) = (worst_diff(x, y), differing_share(x, y, 3.0));
        verdict(
            share < 0.002,
            format!(
                "{a} et {b} : mêmes motifs et mêmes teintes ({:.3} % des pixels à plus de 3/255, écart max {worst}/255)",
                share * 100.0
            ),
        );
        if let Some((oa, ob)) = origins(a, b) {
            verdict(
                oa.to_bits() != ob.to_bits(),
                format!("{a} et {b} : origines différentes ({oa} m, {ob} m)"),
            );
        }
    }
    if ok {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        error!("vérification : échec");
        AppExit::error()
    }
}
