//! Le mode `CF_CHECK=1` de `signs_demo` : une scène par format, deux captures chacune (avec et sans enseignes), verdicts.
//!
//! L'horloge avance d'exactement 1/60 s par image. Le point suivi est immobile : c'est le script qui le place à quelques
//! dizaines de mètres devant le panneau voulu. Les annonces du DJ sont des panneaux **prêtés** (`SignRoad::lease`) : ils
//! passent par le même chemin (cellule cuite, bandeau, matériau) que ceux de la route.

use std::collections::HashMap;
use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::warmup::{AppState, LatePipelines};
use cf_render::{AbsolutePosition, AudioFeatures};
use cf_world_neon::Runner;
use cf_world_neon::signs::road::slot_z;
use cf_world_neon::signs::rig::SignUnitIndex;
use cf_world_neon::signs::show_signs::{LiveAd, LiveRole};
use cf_world_neon::signs::{AdKind, FormatId, SignAd, SignAtlas, SignRoadRes, format_at};

use crate::SIZE;
use crate::shots::{Shot, differing_share};

/// Images laissées à une scène pour que l'origine, les transforms, les tronçons et le matériau soient à jour.
const SETTLE_FRAMES: u32 = 12;
/// Temps réel (s) laissé au démarrage avant de juger que le show n'ouvre jamais.
const READY_TIMEOUT_S: f64 = 120.0;

#[derive(Clone, Copy)]
enum What {
    /// Le premier panneau de ce format sur la route.
    Road(FormatId),
    /// Un panneau « DJ » prêté, posé en face de la voiture.
    Live(FormatId, LiveRole),
}

#[derive(Clone, Copy)]
struct Scene {
    name: &'static str,
    what: What,
    /// Distance entre la voiture et le panneau, en m.
    dist: f64,
}

const SCENES: [Scene; 9] = [
    Scene { name: "1-holo", what: What::Road(FormatId::Holo), dist: 40.0 },
    Scene { name: "2-portique", what: What::Road(FormatId::Gantry), dist: 38.0 },
    Scene { name: "3-neon", what: What::Road(FormatId::Neon), dist: 30.0 },
    Scene { name: "4-projection", what: What::Road(FormatId::Ghost), dist: 90.0 },
    Scene { name: "5-toit", what: What::Road(FormatId::Roof), dist: 110.0 },
    Scene { name: "6-camera", what: What::Road(FormatId::Cam), dist: 45.0 },
    Scene { name: "7-ecran-geant", what: What::Road(FormatId::Jumbo), dist: 120.0 },
    Scene { name: "8-affiche-dj", what: What::Live(FormatId::Holo, LiveRole::Poster), dist: 40.0 },
    Scene { name: "9-ecran-dj", what: What::Live(FormatId::Jumbo, LiveRole::Screen), dist: 70.0 },
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
    ready_at: Option<Instant>,
    /// Indice de scène * 2 + passe (0 : avec enseignes, 1 : sans).
    step: usize,
    stage: Stage,
    leased: Option<usize>,
    shots: HashMap<String, Shot>,
    bake_s: f32,
    /// Pipelines tardifs comptés juste avant la première capture.
    late_before: Option<usize>,
}

fn scene_z(what: What) -> f64 {
    match what {
        What::Road(id) => (2..2_000).map(|k| (k, format_at(k))).find(|(_, f)| *f == id).map_or(-900.0, |(k, _)| slot_z(k)),
        What::Live(..) => -1_500.0,
    }
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    state: Option<Res<State<AppState>>>,
    atlas: Option<Res<SignAtlas>>,
    late: Option<Res<LatePipelines>>,
    mut audio: ResMut<AudioFeatures>,
    mut road: ResMut<SignRoadRes>,
    mut runner: Query<(&mut AbsolutePosition, &mut Visibility), With<Runner>>,
    mut units: Query<&mut Visibility, (With<SignUnitIndex>, Without<Runner>)>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    // Le show ne s'ouvre qu'une fois toutes les textures cuites : `Ready` implique l'atlas.
    let ready = state.is_some_and(|s| *s.get() == AppState::Ready);
    if ready && check.ready_at.is_none() {
        check.ready_at = Some(Instant::now());
        check.bake_s = atlas.as_ref().map_or(f32::NAN, |a| a.bake_s);
        if atlas.is_none() {
            error!("ÉCHEC  Ready sans atlas des enseignes");
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
    let scene_index = check.step / 2;
    let Some(scene) = SCENES.get(scene_index).copied() else {
        exit.write(evaluate(&check, late.as_deref().map_or(0, |l| l.0)));
        return;
    };
    let hide = check.step % 2 == 1;
    let Ok((mut car, mut visibility)) = runner.single_mut() else {
        return;
    };
    match check.stage {
        Stage::Enter => {
            let z = scene_z(scene.what);
            car.0 = DVec3::new(0.0, 0.65, z + scene.dist);
            *visibility = Visibility::Hidden;
            audio.0.pulse = 0.0;
            audio.0.intensity = 0.6;
            if let What::Live(format, role) = scene.what
                && !hide
                && check.leased.is_none()
            {
                let live = LiveAd {
                    role,
                    kind: if role == LiveRole::Poster { AdKind::Estate } else { AdKind::Watch },
                    accent: cf_render::SceneColor::Violet,
                    slot: 0,
                    tag: 0,
                    ghost: false,
                };
                let width = format.format().width;
                if let Some(i) = road.0.lease(format, SignAd::Live(live), width, false, time.elapsed_secs_f64()) {
                    let unit = &mut road.0.units[i];
                    unit.hang = DVec3::new(0.0, f64::from(unit.layout.y), z);
                    unit.layout.yaw = 0.0;
                    unit.layout.x = 0.0;
                    if let Some(state) = unit.state.as_mut() {
                        state.slogan = if role == LiveRole::Poster { 1 } else { 0 };
                    }
                    check.leased = Some(i);
                }
            }
            check.stage = Stage::Settle(0);
        }
        Stage::Settle(frames) if frames < SETTLE_FRAMES => {
            if hide {
                for mut v in &mut units {
                    *v = Visibility::Hidden;
                }
            }
            check.stage = Stage::Settle(frames + 1);
        }
        Stage::Settle(_) => {
            if hide {
                for mut v in &mut units {
                    *v = Visibility::Hidden;
                }
            }
            let name = format!("{}{}", scene.name, if hide { "-vide" } else { "" });
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
        Stage::Await => {
            if hide {
                for mut v in &mut units {
                    *v = Visibility::Hidden;
                }
            }
        }
    }
    // Fin de la passe « avec » d'une scène prêtée : on rend l'unité.
    if check.stage == Stage::Enter
        && check.step % 2 == 1
        && let Some(i) = check.leased.take()
    {
        road.0.unlease(i);
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
    verdict(
        check.bake_s < 5.0,
        format!("cuisson de l'atlas : {:.2} s (critère de la fiche : < 2 s en release)", check.bake_s),
    );
    for scene in &SCENES {
        let (Some(with), Some(without)) = (
            check.shots.get(scene.name),
            check.shots.get(&format!("{}-vide", scene.name)),
        ) else {
            verdict(false, format!("{} : captures absentes", scene.name));
            continue;
        };
        verdict(
            with.width == SIZE.0 && with.height() == SIZE.1,
            format!("{} : capture de {}×{} px", scene.name, with.width, with.height()),
        );
        let share = differing_share(with, without, 8.0);
        verdict(
            share > 0.004,
            format!("{} : l'enseigne est dessinée ({:.2} % des pixels changent)", scene.name, share * 100.0),
        );
    }
    // Le seul pipeline tardif admis est celui du capteur d'écran de la démo (`screenshot-to-screen`), compilé à la première
    // capture : il n'a rien à voir avec le monde.
    let before = check.late_before.unwrap_or(usize::MAX);
    verdict(
        before == 0 && late <= 1,
        format!("aucun pipeline compilé après Ready, enseignes comprises ({before} avant la première capture, {late} à la fin dont celui du capteur d'écran)"),
    );
    if ok {
        info!("tous les verdicts sont verts");
        AppExit::Success
    } else {
        AppExit::error()
    }
}
