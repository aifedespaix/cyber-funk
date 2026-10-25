//! Le mode `CF_CHECK=1` de `landmarks_demo` et la visite interactive.
//!
//! L'horloge avance d'exactement 1/60 s par image. La voiture est immobile ; le script pose un immeuble devant elle
//! ([`LandmarkPass::begin`], le même chemin que l'événement du trafic), le fait vivre quelques dizaines d'images avec un drop
//! (pour passer par les limiteurs), prend deux captures (avec l'immeuble, puis avec sa racine cachée) et juge.

use std::collections::HashMap;
use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::warmup::{AppState, LatePipelines};
use cf_render::{AbsolutePosition, ChunkStream};
use cf_world_neon::landmarks::pass::{LandmarkPass, PassStart};
use cf_world_neon::landmarks::render::LandmarkRig;
use cf_world_neon::landmarks::{LandmarkHandle, LandmarkKind};
use cf_world_neon::traffic::{TrafficBeat, TrafficFrame, TrafficShow};
use cf_world_neon::{NeonWorld, Runner};

use crate::shots::{Shot, differing_share};
use crate::{BPM, SIZE};

/// Images de vie laissées à une scène avant la capture : assez pour que le drop ait monté et que le matériau soit à jour.
const SETTLE_FRAMES: u32 = 36;
/// Image du drop dans la scène.
const DROP_FRAME: u32 = 8;
/// Temps réel (s) laissé au démarrage avant de juger que le show n'ouvre jamais.
const READY_TIMEOUT_S: f64 = 120.0;

/// Une scène : l'immeuble, la distance de son centre devant la voiture, et le côté de la route où la voiture se tient (la
/// façade se lit mieux de biais).
struct Scene {
    kind: LandmarkKind,
    center_m: f32,
    car_x: f64,
    /// Un drop dans la scène (l'arche s'éclate dessus : on la photographie assemblée, sans drop).
    drop: bool,
}

const SCENES: [Scene; 9] = [
    Scene {
        kind: LandmarkKind::Matrix,
        center_m: 100.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Flux,
        center_m: 100.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Binary,
        center_m: 100.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Lava,
        center_m: 100.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Error,
        center_m: 200.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Arcade,
        center_m: 130.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Board,
        center_m: 130.0,
        car_x: -4.0,
        drop: true,
    },
    Scene {
        kind: LandmarkKind::Arch,
        center_m: 90.0,
        car_x: -4.0,
        drop: false,
    },
    Scene {
        kind: LandmarkKind::Mecha,
        center_m: 160.0,
        car_x: -4.0,
        drop: true,
    },
];

/// L'image du trafic d'un immeuble : un kick à 128 BPM, un drop à `DROP_FRAME`, les flashs permis (sous le limiteur).
pub(crate) fn traffic_frame(car_z: f64, time: f64, frame_in_scene: u32) -> TrafficFrame<'static> {
    let beat_s = f64::from(60.0 / BPM);
    let phase = time % beat_s;
    let pulse = (-(phase / 0.12)).exp() as f32;
    TrafficFrame {
        car: DVec3::new(0.0, 0.65, car_z),
        time,
        beat: TrafficBeat {
            kick: phase < 1.0 / 60.0,
            bass: 0.5 * pulse,
            mid: 0.3,
            high: 0.4,
            loud: 0.5,
            pulse,
            count: (time / beat_s) as u32,
            bpm: Some(BPM),
        },
        drive: cf_world_neon::traffic::TrafficDrive {
            drop: frame_in_scene == DROP_FRAME,
            intensity: 0.6,
            section: cf_analysis::Section::Groove,
        },
        show: Some(TrafficShow {
            level: 3,
            breathing: false,
            flashes: true,
            ground_traffic: 0.5,
            event_rate: 1.0,
            events: &[],
            city_pulse: 0.6,
            titles: false,
            dj_seq: 0,
            level_seq: 0,
        }),
        ..TrafficFrame::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Enter,
    Live(u32),
    Await,
}

#[derive(Resource, Default)]
pub(crate) struct Check {
    start: Option<Instant>,
    ready_at: Option<Instant>,
    /// Indice de scène * 2 + passe (0 : avec l'immeuble, 1 : sans).
    step: usize,
    stage: Stage,
    pass: Option<LandmarkPass>,
    shots: HashMap<String, Shot>,
    entities_before: Option<usize>,
    entities_after: usize,
    /// Les scènes jouées (`CF_ONLY=error,mecha` pour en choisir quelques-unes).
    only: Option<Vec<LandmarkKind>>,
    late_before: Option<usize>,
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    state: Option<Res<State<AppState>>>,
    late: Option<Res<LatePipelines>>,
    handle: Res<LandmarkHandle>,
    rig: Option<Res<LandmarkRig>>,
    time: Res<Time>,
    mut runner: Query<(&mut AbsolutePosition, &mut Visibility), With<Runner>>,
    mut roots: Query<&mut Visibility, Without<Runner>>,
    mut world: ResMut<NeonWorld>,
    mut stream: ResMut<ChunkStream<NeonWorld>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    let ready = state.is_some_and(|s| *s.get() == AppState::Ready);
    if ready && check.ready_at.is_none() {
        check.ready_at = Some(Instant::now());
        if rig.is_none() {
            error!("ÉCHEC  Ready sans les entités des immeubles-vitrines");
            exit.write(AppExit::error());
            return;
        }
    }
    let Some(ready_at) = check.ready_at else {
        if start.elapsed().as_secs_f64() > READY_TIMEOUT_S {
            error!("ÉCHEC  le show ne s'ouvre pas ({READY_TIMEOUT_S} s)");
            exit.write(AppExit::error());
        }
        return;
    };
    if ready_at.elapsed().as_secs_f64() < 1.0 {
        return;
    }
    let Some(rig) = rig else { return };
    let only = check
        .only
        .get_or_insert_with(|| {
            let wanted = std::env::var("CF_ONLY").unwrap_or_default();
            SCENES
                .iter()
                .map(|s| s.kind)
                .filter(|k| wanted.is_empty() || wanted.split(',').any(|w| w == k.as_str()))
                .collect()
        })
        .clone();
    let Some(scene) = only
        .get(check.step / 2)
        .and_then(|kind| SCENES.iter().find(|s| s.kind == *kind))
    else {
        check.entities_after = rig.entity_count();
        exit.write(evaluate(&check, late.as_deref().map_or(0, |l| l.0)));
        return;
    };
    let hide = check.step % 2 == 1;
    let Ok((mut car, mut car_visibility)) = runner.single_mut() else {
        return;
    };
    let car_z = 0.0;
    let now = time.elapsed_secs_f64();
    match check.stage {
        Stage::Enter => {
            car.0 = DVec3::new(scene.car_x, 0.65, car_z);
            *car_visibility = Visibility::Hidden;
            if check.entities_before.is_none() {
                check.entities_before = Some(rig.entity_count());
            }
            let mut stage = handle.lock();
            let pass = LandmarkPass::begin(
                &mut stage,
                scene.kind,
                car_z,
                PassStart {
                    side: 1,
                    tint: 2,
                    scale: 1.0,
                    debug_center: Some(scene.center_m),
                },
            );
            // La ville s'écarte devant l'immeuble, comme le fait le directeur du trafic (`apply_clearings`).
            if world.set_clearings(&[pass.clearing()]) {
                stream.replan_all();
            }
            check.pass = Some(pass);
            check.stage = Stage::Live(0);
        }
        Stage::Live(frame) if frame < SETTLE_FRAMES => {
            if let Some(mut pass) = check.pass {
                let mut stage = handle.lock();
                let in_scene = if scene.drop { frame } else { u32::MAX };
                pass.update(&mut stage, &traffic_frame(car_z, now, in_scene), 1.0 / 60.0);
                check.pass = Some(pass);
            }
            // Passe « sans » : la racine est cachée après le système du rendu, avant l'extraction.
            if hide {
                if let Ok(mut visibility) = roots.get_mut(rig.root(scene.kind)) {
                    *visibility = Visibility::Hidden;
                }
            }
            check.stage = Stage::Live(frame + 1);
        }
        Stage::Live(_) => {
            let name = format!("{}{}", scene.kind.as_str(), if hide { "-vide" } else { "" });
            let key = name.clone();
            if check.late_before.is_none() {
                check.late_before = Some(late.as_deref().map_or(0, |l| l.0));
            }
            let mut shot = commands.spawn(Screenshot::primary_window());
            shot.observe(move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>| {
                if let Ok(image) = captured.image.clone().try_into_dynamic() {
                    let rgba = image.to_rgba8();
                    let width = rgba.width();
                    check.shots.insert(
                        key.clone(),
                        Shot {
                            width,
                            rgba: rgba.into_raw(),
                        },
                    );
                }
                check.step += 1;
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
    // Fin de la passe « sans » : on range l'immeuble.
    if check.stage == Stage::Enter && check.step % 2 == 0 && check.step > 0 {
        if let Some(mut pass) = check.pass.take() {
            let mut stage = handle.lock();
            pass.end(&mut stage);
            if world.set_clearings(&[]) {
                stream.replan_all();
            }
        }
    }
}

fn evaluate(check: &Check, late: usize) -> AppExit {
    let mut ok = true;
    let mut verdict = |pass: bool, what: String| {
        if pass {
            info!("OK     {what}");
        } else {
            error!("ÉCHEC  {what}");
            ok = false;
        }
    };
    for scene in SCENES
        .iter()
        .filter(|s| check.only.as_ref().is_none_or(|o| o.contains(&s.kind)))
    {
        let name = scene.kind.as_str();
        let (Some(with), Some(without)) = (check.shots.get(name), check.shots.get(&format!("{name}-vide"))) else {
            verdict(false, format!("{name} : captures absentes"));
            continue;
        };
        verdict(
            with.width == SIZE.0 && with.height() == SIZE.1,
            format!("{name} : capture de {}×{} px", with.width, with.height()),
        );
        let share = differing_share(with, without, 8.0);
        verdict(
            share > 0.004,
            format!(
                "{name} : l'immeuble est dessiné ({:.2} % des pixels changent)",
                share * 100.0
            ),
        );
    }
    verdict(
        check.entities_before == Some(check.entities_after),
        format!(
            "aucune entité créée pendant le show ({:?} avant, {} après)",
            check.entities_before, check.entities_after
        ),
    );
    // Le seul pipeline tardif admis est celui du capteur d'écran de la démo (`screenshot-to-screen`), compilé à la première
    // capture : il n'a rien à voir avec le monde.
    let before = check.late_before.unwrap_or(usize::MAX);
    verdict(
        before == 0 && late <= 1,
        format!(
            "aucun pipeline compilé après Ready, immeubles compris ({before} avant la première capture, {late} à la fin dont celui du capteur d'écran)"
        ),
    );
    if ok {
        info!("tous les verdicts sont verts");
        AppExit::Success
    } else {
        AppExit::error()
    }
}

/// La visite interactive : un immeuble après l'autre devant la voiture qui roule.
#[derive(Resource, Default)]
pub(crate) struct Tour {
    index: usize,
    pass: Option<LandmarkPass>,
}

pub(crate) fn tour(
    mut tour: ResMut<Tour>,
    handle: Res<LandmarkHandle>,
    time: Res<Time>,
    car: Query<&AbsolutePosition, With<Runner>>,
) {
    let Ok(car) = car.single() else { return };
    let mut stage = handle.lock();
    let now = time.elapsed_secs_f64();
    let mut running = tour.pass.take();
    if running.is_none_or(|p| p.finished(car.z)) {
        if let Some(mut done) = running {
            done.end(&mut stage);
        }
        let kind = SCENES[tour.index % SCENES.len()].kind;
        tour.index += 1;
        running = Some(LandmarkPass::begin(
            &mut stage,
            kind,
            car.z,
            PassStart {
                side: if tour.index % 2 == 0 { 1 } else { -1 },
                tint: 2,
                scale: 1.0,
                debug_center: None,
            },
        ));
    }
    if let Some(mut pass) = running {
        pass.update(&mut stage, &traffic_frame(car.z, now, u32::MAX), time.delta_secs());
        tour.pass = Some(pass);
    }
}
