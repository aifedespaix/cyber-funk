//! Le mode `CF_CHECK=1` de `city_demo` : scènes scriptées, captures, compteurs de culling, verdicts sur les pixels.
//!
//! L'horloge avance d'exactement 1/60 s par image et le temps du shader est imposé (`FacadeLook::clock_s`) : la capture d'une
//! scène ne dépend ni de la vitesse de la machine, ni du moment où elle tombe. Le point suivi est immobile : c'est le
//! script qui le téléporte.

use std::fmt::Write as _;
use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::{AbsolutePosition, AudioFeatures, DiagnosticsSnapshot, FloatingOrigin};
use cf_world_neon::{FacadeLook, RoadLook, Runner};

use crate::SIZE;
use crate::shots::{Shot, differing_share, luma, worst_diff};

/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 10.0;
/// Images laissées à une scène pour que l'origine, les transforms, les tronçons et le matériau soient à jour.
const SETTLE_FRAMES: u32 = 10;

#[derive(Clone, Copy)]
struct Scene {
    name: &'static str,
    via_z: Option<f64>,
    car_z: f64,
    time_s: f64,
    pulse: f32,
    beat: f64,
    wet: f32,
    rain: f32,
    build: f32,
}

const BASE: Scene = Scene {
    name: "",
    via_z: None,
    car_z: -1234.5,
    time_s: 3.0,
    pulse: 0.0,
    beat: 4.0,
    wet: 0.0,
    rain: 0.0,
    build: 1.0,
};

const SCENES: [Scene; 7] = [
    Scene {
        name: "1-depart",
        ..BASE
    },
    Scene {
        name: "2-kick",
        pulse: 1.0,
        beat: 4.0,
        ..BASE
    },
    Scene {
        name: "3-pluie",
        wet: 1.0,
        rain: 0.8,
        time_s: 12.0,
        ..BASE
    },
    Scene {
        name: "4-revelation",
        build: 0.45,
        ..BASE
    },
    // À 110 m de la première arche (z = −1 203,4 m) : plus loin, la brume l'a déjà avalée (elle est pleine à 380 m).
    Scene {
        name: "5-arche",
        car_z: -1203.4 + 110.0,
        ..BASE
    },
    // Même point de vue de part et d'autre d'un recentrage d'origine (28 800 m) : la ville doit être identique.
    Scene {
        name: "6-seuil-direct",
        car_z: -14_000.0,
        ..BASE
    },
    Scene {
        name: "7-seuil-via-km30",
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
    shots: std::collections::HashMap<&'static str, Shot>,
    counters: String,
    origins: std::collections::HashMap<&'static str, f64>,
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut audio: ResMut<AudioFeatures>,
    mut road: ResMut<RoadLook>,
    mut facade: ResMut<FacadeLook>,
    origin: Res<FloatingOrigin>,
    snapshot: Res<DiagnosticsSnapshot>,
    mut runner: Query<(&mut AbsolutePosition, &mut Visibility), With<Runner>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    let Some(scene) = SCENES.get(check.index).copied() else {
        if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
            let _ = std::fs::write(
                std::path::Path::new(&dir).join("counters.json"),
                format!("[{}]\n", check.counters),
            );
        }
        exit.write(evaluate(&check));
        return;
    };
    let Ok((mut car, mut visibility)) = runner.single_mut() else {
        return;
    };
    match check.stage {
        Stage::Enter => {
            let z = match scene.via_z {
                Some(via) if !check.via_done => {
                    check.via_done = true;
                    via
                }
                _ => scene.car_z,
            };
            car.0 = DVec3::new(0.0, 0.65, z);
            *visibility = Visibility::Hidden;
            audio.0.pulse = scene.pulse;
            audio.0.bass = 0.5 * scene.pulse;
            audio.0.high = 0.4;
            audio.0.mid = 0.3;
            audio.0.beat = scene.beat;
            audio.0.intensity = 0.7;
            road.wet = scene.wet;
            road.rain = scene.rain;
            facade.build = scene.build;
            facade.clock_s = Some(scene.time_s);
            road.clock_s = Some(scene.time_s);
            check.stage = Stage::Settle(0);
        }
        Stage::Settle(frames) if frames < SETTLE_FRAMES => check.stage = Stage::Settle(frames + 1),
        Stage::Settle(_) => {
            if scene.via_z.is_some() && check.via_done && car.0.z.to_bits() != scene.car_z.to_bits() {
                car.0.z = scene.car_z;
                check.stage = Stage::Settle(0);
                return;
            }
            let stream = snapshot.stream.unwrap_or_default();
            let comma = if check.counters.is_empty() { "" } else { "," };
            let _ = write!(
                check.counters,
                "{comma}\n {{\"scene\":\"{}\",\"pieces_pooled\":{},\"pieces_used\":{},\"renderable\":{},\"visible\":{},\"mesh_kinds\":{},\"frame_ms\":{:.2}}}",
                scene.name,
                stream.pieces_pooled,
                stream.pieces_used,
                snapshot.renderable_entities,
                snapshot.visible_entities,
                snapshot.visible_mesh_kinds,
                snapshot.frame_ms,
            );
            info!(
                "{} : {} pièces préallouées, {} affichées par les tronçons, {} entités dessinables, {} visibles après culling",
                scene.name,
                stream.pieces_pooled,
                stream.pieces_used,
                snapshot.renderable_entities,
                snapshot.visible_entities
            );
            let name = scene.name;
            check.origins.insert(name, origin.origin().z);
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
            }
            check.stage = Stage::Await;
        }
        Stage::Await => {}
    }
}

/// Le haut de l'écran : là où sont les façades (la route est dans le tiers bas).
const SKY_AND_TOWERS: ((u32, u32), (u32, u32)) = ((0, 0), (960, 330));

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
    let lit = |p: [f32; 3]| luma(p) > 40.0;
    let windows = depart.count_in(SKY_AND_TOWERS, lit);
    verdict(
        windows > 600,
        format!("les façades sont dessinées ({windows} pixels lumineux en hauteur)"),
    );
    let (calm, pulse) = (
        depart.mean_luma(SKY_AND_TOWERS.0, SKY_AND_TOWERS.1),
        kick.mean_luma(SKY_AND_TOWERS.0, SKY_AND_TOWERS.1),
    );
    verdict(
        pulse > calm + 0.3,
        format!("le kick fait pulser les façades ({calm:.2} → {pulse:.2})"),
    );
    if let Some(rain) = shot("3-pluie") {
        let share = differing_share(depart, rain, 8.0);
        verdict(
            share > 0.05,
            format!("pluie : la ville change ({:.0} % des pixels)", share * 100.0),
        );
    }
    if let Some(intro) = shot("4-revelation") {
        let (full, cut) = (
            depart.count_in(SKY_AND_TOWERS, lit),
            intro.count_in(SKY_AND_TOWERS, lit),
        );
        verdict(
            cut < full,
            format!("révélation : la ville est coupée en hauteur ({full} → {cut} pixels lumineux)"),
        );
    }
    if let Some(bridge) = shot("5-arche") {
        let arch = bridge.count_in(SKY_AND_TOWERS, lit);
        verdict(
            arch > 600,
            format!("arche : la scène est dessinée ({arch} pixels lumineux)"),
        );
    }
    if let (Some(a), Some(b)) = (shot("6-seuil-direct"), shot("7-seuil-via-km30")) {
        let (worst, share) = (worst_diff(a, b), differing_share(a, b, 3.0));
        verdict(
            share < 0.002,
            format!(
                "recentrage d'origine : même ville ({:.3} % des pixels à plus de 3/255, écart max {worst}/255)",
                share * 100.0
            ),
        );
        let (oa, ob) = (
            check.origins.get("6-seuil-direct"),
            check.origins.get("7-seuil-via-km30"),
        );
        verdict(
            oa != ob,
            format!("recentrage d'origine : origines différentes ({oa:?}, {ob:?})"),
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
