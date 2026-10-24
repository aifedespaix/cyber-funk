//! Le mode `CF_CHECK=1` de `sky_demo` : scènes scriptées, captures, verdicts sur les pixels.
//!
//! L'horloge avance d'exactement 1/60 s par image et le temps du shader est imposé (`SkyLook::clock_s`) : la capture d'une
//! scène ne dépend ni de la vitesse de la machine, ni du moment où elle tombe.

use std::time::Instant;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::{AudioFeatures, CameraPose};
use cf_world_neon::sky::SkyState;
use cf_world_neon::{RingPlanet, ShootingStarRequest, SkyLook};

use crate::shots::{Shot, differing_share, luma, worst_diff};
use crate::{SIZE, look_at, pose_for};

/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 10.0;
/// Images laissées à une scène pour que la pose, les transforms et les matériaux soient à jour.
const SETTLE_FRAMES: u32 = 10;
/// Images de plus pour la scène de l'étoile filante : le second ordre part à l'image 16, le trait a le temps de se dessiner.
const SHOOT_EXTRA_FRAMES: u32 = 14;
/// Image où part le second ordre de la scène de l'étoile filante.
const SECOND_SHOT_FRAME: u32 = 16;

#[derive(Clone, Copy)]
struct Scene {
    name: &'static str,
    /// Orientation : vers la géante à anneau (sinon `yaw`, `pitch`).
    at_giant: bool,
    yaw_deg: f32,
    pitch_deg: f32,
    /// Position de la caméra dans le monde, en m (z).
    z_m: f64,
    /// Part du voyage vers la géante, 0..1.
    drift: f32,
    pulse: f32,
    reveal: f32,
    time_s: f64,
    shoot: bool,
}

const BASE: Scene = Scene {
    name: "",
    at_giant: false,
    yaw_deg: 0.0,
    pitch_deg: 20.0,
    z_m: 0.0,
    drift: 0.0,
    pulse: 0.0,
    reveal: 1.0,
    time_s: 3.0,
    shoot: false,
};

const SCENES: [Scene; 10] = [
    Scene { name: "1-face", ..BASE },
    Scene {
        name: "2-geante",
        at_giant: true,
        ..BASE
    },
    Scene {
        name: "3-voyage",
        at_giant: true,
        drift: 0.85,
        ..BASE
    },
    Scene {
        name: "4-kick",
        pulse: 1.0,
        ..BASE
    },
    Scene {
        name: "5-fondu",
        reveal: 0.3,
        ..BASE
    },
    // Même point de vue au km 400 : le ciel doit être identique (il est à l'infini).
    Scene {
        name: "6-km400",
        z_m: -400_000.0,
        ..BASE
    },
    // Même vue deux fois, avec puis sans étoile filante. Le premier départ de la graine de Sillage naît juste derrière la
    // géante : on en lance deux (l'écart minimal est de 0,25 s) et la caméra regarde à droite, où part le second.
    Scene {
        name: "7-sans-filante",
        yaw_deg: -45.0,
        pitch_deg: 30.0,
        ..BASE
    },
    Scene {
        name: "8-filante",
        yaw_deg: -45.0,
        pitch_deg: 30.0,
        shoot: true,
        ..BASE
    },
    Scene {
        name: "9-noir",
        reveal: 0.0,
        ..BASE
    },
    Scene {
        name: "10-voie-lactee",
        yaw_deg: 150.0,
        pitch_deg: 35.0,
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
    shots: std::collections::HashMap<&'static str, Shot>,
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut audio: ResMut<AudioFeatures>,
    mut look: ResMut<SkyLook>,
    mut sky: ResMut<SkyState>,
    mut pose: ResMut<CameraPose>,
    giant: Option<Res<RingPlanet>>,
    mut shoot: MessageWriter<ShootingStarRequest>,
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
    match check.stage {
        Stage::Enter => {
            let (yaw, pitch) = match (scene.at_giant, giant.as_deref()) {
                (true, Some(g)) => look_at(g.center),
                _ => (scene.yaw_deg.to_radians(), scene.pitch_deg.to_radians()),
            };
            *pose = pose_for(yaw, pitch, scene.z_m, scene.drift, giant.as_deref());
            // Chaque scène repart du même instant : mêmes astres au même angle, pas d'étoile filante en vol.
            sky.reset();
            audio.0.pulse = scene.pulse;
            look.reveal = scene.reveal;
            look.clock_s = Some(scene.time_s);
            if scene.shoot {
                shoot.write(ShootingStarRequest);
            }
            check.stage = Stage::Settle(0);
        }
        Stage::Settle(frames) if frames < SETTLE_FRAMES + if scene.shoot { SHOOT_EXTRA_FRAMES } else { 0 } => {
            // Le second ordre part 16 images (0,27 s) après le premier : après l'écart minimal de la cadence.
            if scene.shoot && frames == SECOND_SHOT_FRAME {
                shoot.write(ShootingStarRequest);
            }
            check.stage = Stage::Settle(frames + 1);
        }
        Stage::Settle(_) => {
            let name = scene.name;
            let flying = sky.shooting().streaks().iter().filter(|s| !s.is_free()).count();
            info!("{name} : {flying} étoile(s) filante(s) en vol");
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

const FULL: ((u32, u32), (u32, u32)) = ((0, 0), (SIZE.0, SIZE.1));
const CENTER: ((u32, u32), (u32, u32)) = ((SIZE.0 / 4, SIZE.1 / 4), (SIZE.0 * 3 / 4, SIZE.1 * 3 / 4));

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
    let Some(face) = shot("1-face") else {
        error!("capture de départ absente");
        return AppExit::error();
    };
    verdict(
        face.width == SIZE.0 && face.height() == SIZE.1,
        format!("capture de {}×{} px", face.width, face.height()),
    );
    let mean = face.mean_luma(FULL.0, FULL.1);
    verdict(mean > 1.0, format!("le ciel est dessiné (luminance moyenne {mean:.2})"));
    let stars = face.count(|p| luma(p) > 60.0);
    verdict(stars > 150, format!("des étoiles brillent ({stars} pixels > 60/255)"));
    if let Some(milky) = shot("10-voie-lactee") {
        let (a, b) = (face.mean_luma(FULL.0, FULL.1), milky.mean_luma(FULL.0, FULL.1));
        verdict(
            (a - b).abs() > 0.05 || differing_share(face, milky, 10.0) > 0.05,
            format!("deux directions du ciel ne se ressemblent pas ({a:.2} / {b:.2})"),
        );
    }
    if let Some(giant) = shot("2-geante") {
        let lit = giant.count_in(CENTER, |p| luma(p) > 25.0);
        verdict(
            lit > 800,
            format!("la géante à anneau est au centre ({lit} pixels lumineux)"),
        );
        if let Some(trip) = shot("3-voyage") {
            let near = trip.count_in(CENTER, |p| luma(p) > 25.0);
            verdict(
                near as f32 > lit as f32 * 1.2,
                format!("sky_drift : la géante grossit quand le ciel voyage ({lit} → {near} pixels)"),
            );
        }
    }
    if let Some(kick) = shot("4-kick") {
        let (calm, pulse) = (face.mean_luma(FULL.0, FULL.1), kick.mean_luma(FULL.0, FULL.1));
        verdict(
            pulse > calm,
            format!("le kick éclaire légèrement le ciel ({calm:.2} → {pulse:.2})"),
        );
    }
    if let Some(fade) = shot("5-fondu") {
        let faded = fade.mean_luma(FULL.0, FULL.1);
        verdict(
            faded < mean * 0.6,
            format!("fondu à 30 % : le ciel s'assombrit ({mean:.2} → {faded:.2})"),
        );
    }
    if let Some(black) = shot("9-noir") {
        let m = black.mean_luma(FULL.0, FULL.1);
        verdict(m < 1.0, format!("fondu à 0 : ciel noir (luminance moyenne {m:.2})"));
    }
    if let Some(far) = shot("6-km400") {
        let (worst, share) = (worst_diff(face, far), differing_share(face, far, 12.0));
        verdict(
            share < 0.002,
            format!(
                "km 400 : même ciel qu'au départ ({:.3} % des pixels à plus de 12/255, écart max {worst}/255)",
                share * 100.0
            ),
        );
    }
    if let (Some(without), Some(streak)) = (shot("7-sans-filante"), shot("8-filante")) {
        let changed = without.count_pairs(streak, |p, q| (0..3).any(|c| (p[c] - q[c]).abs() > 25.0));
        verdict(
            changed > 40,
            format!("étoile filante : un trait apparaît ({changed} pixels changés)"),
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
