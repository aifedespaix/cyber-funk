//! Le mode `CF_CHECK=1` de `beams_demo` : la scène est figée (voiture immobile, audio constant), le script photographie la rue
//! sans puis avec les faisceaux, puis le sol autour de la voiture juste avant le drop, **sur l'image du drop**, 12 images et
//! 30 images plus tard. Verdicts sur les pixels, sur la compilation des shaders et sur la règle 8.
//!
//! L'horloge avance d'exactement 1/60 s par image et s'arrête le temps de chaque capture.

use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::warmup::{AppState, LatePipelines};
use cf_render::{CameraPose, CurrentShowFx};
use cf_world_neon::beams::BeamPart;
use cf_world_neon::dropfx::DropFx;

use crate::shots::{Shot, differing_share};
use crate::{CAR, PACKAGE, pose_to, show_fx};

/// Temps réel (s) laissé à l'ouverture du show avant d'abandonner.
const READY_TIMEOUT_S: f64 = 90.0;
/// Images laissées aux faisceaux pour se placer après un changement.
const SETTLE_FRAMES: u32 = 4;
/// Images entre le drop et la capture de l'onde, puis de la cascade (0,2 s et 0,5 s).
const WAVE_FRAME: u32 = 12;
const CASCADE_FRAME: u32 = 30;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Ready,
    /// Rue sans faisceaux (cachés par la démo).
    Bare {
        frames: u32,
    },
    /// Rue avec faisceaux.
    Beams {
        frames: u32,
    },
    /// Le sol autour de la voiture, avant le drop.
    Before {
        frames: u32,
    },
    /// Le drop vient de partir (image 0), puis les captures suivantes.
    Drop {
        frames: u32,
    },
    Done,
}

#[derive(Resource)]
pub(crate) struct Check {
    flashes: bool,
    start: Option<Instant>,
    phase: Phase,
    awaiting: bool,
    clear_drop: bool,
    hide_beams: bool,
    shots: HashMap<&'static str, Shot>,
    late_before: Option<usize>,
    drop_frame_matches: Option<bool>,
    visible_at_drop: Option<(usize, bool)>,
}

impl Check {
    pub(crate) fn new(flashes: bool) -> Self {
        Self {
            flashes,
            start: None,
            phase: Phase::Ready,
            awaiting: false,
            clear_drop: false,
            hide_beams: true,
            shots: HashMap::new(),
            late_before: None,
            drop_frame_matches: None,
            visible_at_drop: None,
        }
    }
}

/// Fige l'horloge le temps de la capture (le rendu logiciel met des secondes à la rendre).
fn capture(commands: &mut Commands, time: &mut Time<Virtual>, name: &'static str) {
    time.pause();
    let mut shot = commands.spawn(Screenshot::primary_window());
    shot.observe(
        move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>, mut time: ResMut<Time<Virtual>>| {
            time.unpause();
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
            check.awaiting = false;
        },
    );
    if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
    }
}

/// Après `CfSet::Present` : cache les faisceaux pendant la capture « sans » (la scène est sinon identique).
pub(crate) fn hide_beams(check: Res<Check>, mut parts: Query<&mut Visibility, With<BeamPart>>) {
    if check.hide_beams {
        for mut visibility in &mut parts {
            *visibility = Visibility::Hidden;
        }
    }
}

/// La rue vue de derrière la voiture, un peu haute.
fn street() -> CameraPose {
    pose_to(
        CAR + bevy::math::DVec3::new(0.0, 9.0, 25.0),
        CAR + bevy::math::DVec3::new(0.0, 8.0, -90.0),
    )
}

/// Le sol autour de la voiture.
fn ground() -> CameraPose {
    pose_to(
        CAR + bevy::math::DVec3::new(7.0, 4.5, 10.0),
        CAR + bevy::math::DVec3::new(0.0, 0.5, 0.0),
    )
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    state: Option<Res<State<AppState>>>,
    late: Option<Res<LatePipelines>>,
    dropfx: Res<DropFx>,
    mut fx: ResMut<CurrentShowFx>,
    mut features: ResMut<cf_render::AudioFeatures>,
    mut pose: ResMut<CameraPose>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    // Audio constant : seul le paquet du drop change la scène entre deux captures.
    features.0.pulse = 0.0;
    features.0.mid = 0.4;
    features.0.beat = 8.0;
    // Le paquet n'existe que sur une image : il est retiré dès l'image suivante, captures en attente ou non.
    if check.clear_drop {
        check.clear_drop = false;
        // Début de l'image qui suit le drop : le dernier pas de `DropFx` est celui de l'image du drop.
        check.drop_frame_matches = Some(dropfx.packages() == 1 && dropfx.last_package_frame() == Some(dropfx.frame()));
        check.visible_at_drop = Some((dropfx.rings_alive(), dropfx.particles_alive() > 0));
        *fx = show_fx(check.flashes, None);
    }
    if check.awaiting {
        return;
    }
    match check.phase {
        Phase::Ready => {
            if state.is_some_and(|s| *s.get() == AppState::Ready) {
                check.phase = Phase::Bare { frames: 0 };
            } else if start.elapsed().as_secs_f64() > READY_TIMEOUT_S {
                error!("ÉCHEC  le show ne s'ouvre pas ({READY_TIMEOUT_S} s)");
                exit.write(AppExit::error());
            }
        }
        Phase::Bare { frames } => {
            *pose = street();
            check.phase = Phase::Bare { frames: frames + 1 };
            // Une seconde de réel après `Ready` : de quoi finir le chargement des tronçons.
            if frames == 60 {
                check.late_before = Some(late.as_deref().map_or(0, |l| l.0));
                check.awaiting = true;
                capture(&mut commands, &mut time, "0-sans-faisceaux");
                check.phase = Phase::Beams { frames: 0 };
            }
        }
        Phase::Beams { frames } => {
            *pose = street();
            check.hide_beams = false;
            check.phase = Phase::Beams { frames: frames + 1 };
            if frames == SETTLE_FRAMES {
                check.awaiting = true;
                capture(&mut commands, &mut time, "1-faisceaux");
                check.phase = Phase::Before { frames: 0 };
            }
        }
        Phase::Before { frames } => {
            *pose = ground();
            check.phase = Phase::Before { frames: frames + 1 };
            if frames == SETTLE_FRAMES {
                check.awaiting = true;
                capture(&mut commands, &mut time, "2-avant-drop");
                check.phase = Phase::Drop { frames: 0 };
            }
        }
        Phase::Drop { frames } => {
            *pose = ground();
            if frames == 0 {
                // `ShowFxInput::drop` n'existe que sur cette image, comme `drop_now`. `step_dropfx` (CfSet::Simulate) la lit
                // dans cette même image, `sync_dropfx` (CfSet::Present) la montre, la capture la photographie.
                *fx = show_fx(check.flashes, Some(PACKAGE));
                check.clear_drop = true;
                check.awaiting = true;
                capture(&mut commands, &mut time, "3-drop");
            }
            if frames == WAVE_FRAME {
                check.awaiting = true;
                capture(&mut commands, &mut time, "4-onde");
            }
            if frames == CASCADE_FRAME {
                check.awaiting = true;
                capture(&mut commands, &mut time, "5-cascade");
                check.phase = Phase::Done;
                return;
            }
            check.phase = Phase::Drop { frames: frames + 1 };
        }
        Phase::Done => {
            exit.write(evaluate(&check, late.as_deref().map_or(0, |l| l.0)));
        }
    }
}

/// Le sol autour de la voiture : la partie basse de l'image, sous les projecteurs qui balaient entre deux captures.
const GROUND: ((u32, u32), (u32, u32)) = ((150, 290), (750, 540));

/// Part des pixels du sol qui diffèrent de plus de `level` (sur 255) entre deux captures.
fn ground_share(a: &Shot, b: &Shot, level: f32) -> f32 {
    let ((x0, y0), (x1, y1)) = GROUND;
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let (p, q) = (a.pixel(x, y), b.pixel(x, y));
            if (0..3).any(|c| (p[c] - q[c]).abs() > level) {
                n += 1;
            }
        }
    }
    n as f32 / ((x1 - x0) * (y1 - y0)) as f32
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
    let shot = |name: &str| check.shots.get(name);
    if let (Some(bare), Some(beams)) = (shot("0-sans-faisceaux"), shot("1-faisceaux")) {
        let share = differing_share(bare, beams, 12.0);
        verdict(
            share > 0.002,
            format!(
                "les faisceaux sont dessinés ({:.2} % des pixels changent)",
                share * 100.0
            ),
        );
    } else {
        verdict(false, "captures des faisceaux absentes".into());
    }
    verdict(
        check.drop_frame_matches == Some(true),
        format!(
            "le paquet est parti dans l'image du drop (état lu à l'image suivante : {:?})",
            check.drop_frame_matches
        ),
    );
    if let (Some(before), Some(drop), Some(wave), Some(cascade)) =
        (shot("2-avant-drop"), shot("3-drop"), shot("4-onde"), shot("5-cascade"))
    {
        let at_drop = ground_share(before, drop, 12.0);
        let at_wave = ground_share(before, wave, 12.0);
        let at_cascade = ground_share(before, cascade, 12.0);
        if check.flashes {
            verdict(
                at_drop > 0.001,
                format!(
                    "la gerbe est à l'écran sur l'image du drop ({:.3} % des pixels)",
                    at_drop * 100.0
                ),
            );
            verdict(
                at_wave > 0.02,
                format!(
                    "l'onde et la gerbe sont visibles 12 images après ({:.2} % des pixels)",
                    at_wave * 100.0
                ),
            );
            verdict(
                at_cascade > 0.02,
                format!(
                    "la cascade est encore à l'écran 30 images après ({:.2} % des pixels)",
                    at_cascade * 100.0
                ),
            );
            verdict(
                check
                    .visible_at_drop
                    .is_some_and(|(rings, sparks)| rings >= 1 && sparks),
                format!("onde et gerbe vivantes à l'image du drop ({:?})", check.visible_at_drop),
            );
        } else {
            verdict(
                at_drop < 0.0005 && at_wave < 0.0005 && at_cascade < 0.0005,
                format!(
                    "flashs coupés : le drop ne montre rien ({:.4} %, {:.4} %, {:.4} % des pixels)",
                    at_drop * 100.0,
                    at_wave * 100.0,
                    at_cascade * 100.0
                ),
            );
        }
    } else {
        verdict(false, "captures du drop absentes".into());
    }
    // Le seul pipeline tardif admis est celui du capteur d'écran de la démo (`screenshot-to-screen`), compilé à la première
    // capture : il n'a rien à voir avec le monde.
    let before = check.late_before.unwrap_or(usize::MAX);
    verdict(
        before == 0 && late <= 1,
        format!(
            "aucun pipeline compilé après Ready, drop compris ({before} avant la première capture, {late} à la fin dont celui du capteur d'écran)"
        ),
    );
    if ok {
        info!("tous les verdicts sont verts");
        AppExit::Success
    } else {
        AppExit::error()
    }
}
