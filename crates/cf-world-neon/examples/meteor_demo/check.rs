//! Le mode `CF_CHECK=1` de `meteor_demo` : un astre suivi en vol, puis sa fin (explosion contre la ville, ou anneau dans la
//! brume sans ville), une capture à chaque étape et des verdicts sur les pixels et sur le limiteur de flashs.
//!
//! L'horloge avance d'exactement 1/60 s par image : les astres, tirés par graine, tombent toujours pareil.

use std::collections::HashMap;
use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::CameraPose;
use cf_show::limiter::{MAX_FLASH_PEAK, MIN_FADE};
use cf_world_neon::Meteors;
use cf_world_neon::sky::PlanetTextures;

use crate::shots::{Shot, differing_share, luma};
use crate::{CAR, SIZE, pose_to};

/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 12.0;
/// Image de vol où l'on capture l'astre.
const FLIGHT_SHOT_FRAME: u32 = 40;
/// Combien d'astres on essaie avant de renoncer à trouver un mur (avec ville).
const MAX_TRIES: u32 = 24;
/// Images après la fin de l'astre, et nom de la capture : avec ville.
const BLAST_SHOTS: [(u32, &str); 4] = [(8, "2-eclair"), (36, "3-onde"), (90, "4-debris"), (240, "5-apres")];
/// Idem sans ville : l'anneau de brume.
const SPLASH_SHOTS: [(u32, &str); 3] = [(12, "2-anneau"), (60, "3-anneau-etale"), (200, "4-apres")];

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Warmup,
    Flight { id: u64, frames: u32 },
    Aftermath { at: DVec3, frames: u32, blast: bool },
    Done,
}

#[derive(Resource)]
pub(crate) struct Check {
    city: bool,
    start: Option<Instant>,
    phase: Phase,
    tries: u32,
    awaiting: bool,
    shots: HashMap<&'static str, Shot>,
    levels: Vec<f32>,
}

impl Check {
    pub(crate) fn new(city: bool) -> Self {
        Self {
            city,
            start: None,
            phase: Phase::Warmup,
            tries: 0,
            awaiting: false,
            shots: HashMap::new(),
            levels: Vec::new(),
        }
    }
}

/// Fige l'horloge le temps de la capture (le rendu logiciel met des secondes à la rendre) : la scène photographiée est celle
/// de l'image où l'on demande, et le script ne rate pas la fin de l'astre.
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

pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut meteors: ResMut<Meteors>,
    textures: Option<Res<PlanetTextures>>,
    mut pose: ResMut<CameraPose>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    check.levels.push(meteors.flash().level());
    if check.awaiting {
        return;
    }
    let count = textures.map_or(0, |t| t.0.len());
    match check.phase {
        Phase::Warmup => {
            let id = meteors.launch(CAR, false, count);
            check.tries += 1;
            check.phase = id.map_or(Phase::Done, |id| Phase::Flight { id, frames: 0 });
        }
        Phase::Flight { id, frames } => {
            if let Some(m) = meteors.field().active().find(|m| m.id == id).copied() {
                // Derrière l'astre, un peu au-dessus : il file vers le fond.
                let eye = m.position - m.velocity.as_dvec3().normalize() * 45.0 + DVec3::new(6.0, 9.0, 0.0);
                *pose = pose_to(eye, m.position);
                check.phase = Phase::Flight { id, frames: frames + 1 };
                if frames == FLIGHT_SHOT_FRAME {
                    check.awaiting = true;
                    capture(&mut commands, &mut time, "1-vol");
                }
                return;
            }
            // L'astre vient de finir.
            let blast = meteors.last_blast().filter(|b| b.meteor_id == id);
            let splash = meteors
                .field()
                .splashes()
                .iter()
                .find(|s| !s.is_free())
                .map(|s| s.position);
            check.phase = match (blast, splash, check.city) {
                (Some(b), _, true) => Phase::Aftermath {
                    at: b.at,
                    frames: 0,
                    blast: true,
                },
                (None, Some(at), false) => Phase::Aftermath {
                    at,
                    frames: 0,
                    blast: false,
                },
                // Pas la fin qu'on veut filmer (sans ville, `NeonWorld::collides` garde les immeubles invisibles) : un autre.
                _ if check.tries < MAX_TRIES => {
                    check.tries += 1;
                    meteors
                        .launch(CAR, false, count)
                        .map_or(Phase::Done, |id| Phase::Flight {
                            id,
                            frames: FLIGHT_SHOT_FRAME + 1,
                        })
                }
                _ => Phase::Done,
            };
        }
        Phase::Aftermath { at, frames, blast } => {
            let eye = at + DVec3::new(30.0, 14.0, 80.0);
            *pose = pose_to(eye, at + DVec3::new(0.0, if blast { 6.0 } else { 0.0 }, 0.0));
            let plan: &[(u32, &'static str)] = if blast { &BLAST_SHOTS } else { &SPLASH_SHOTS };
            check.phase = Phase::Aftermath {
                at,
                frames: frames + 1,
                blast,
            };
            if let Some(&(_, name)) = plan.iter().find(|(f, _)| *f == frames) {
                check.awaiting = true;
                capture(&mut commands, &mut time, name);
            }
            if frames > plan.last().map_or(0, |p| p.0) + 5 {
                check.phase = Phase::Done;
            }
        }
        Phase::Done => {
            exit.write(evaluate(&check));
        }
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
    match shot("1-vol") {
        Some(flight) => {
            let lit = flight.count_in(CENTER, |p| luma(p) > 60.0);
            verdict(
                lit > 150,
                format!("un astre brille au centre de l'image ({lit} pixels > 60/255)"),
            );
        }
        None => verdict(false, "capture du vol absente".into()),
    }
    if check.city {
        match (shot("2-eclair"), shot("5-apres")) {
            (Some(flash), Some(after)) => {
                let bright = flash.count_in(CENTER, |p| luma(p) > 90.0);
                let changed = differing_share(flash, after, 25.0);
                verdict(
                    changed > 0.003,
                    format!(
                        "l'explosion change l'image ({:.2} % des pixels, {bright} pixels > 90/255)",
                        changed * 100.0
                    ),
                );
            }
            _ => verdict(false, "captures de l'explosion absentes".into()),
        }
        if let (Some(shock), Some(after)) = (shot("3-onde"), shot("5-apres")) {
            let changed = differing_share(shock, after, 25.0);
            verdict(
                changed > 0.003,
                format!("l'onde de choc s'étale ({:.2} % des pixels)", changed * 100.0),
            );
        }
        if let (Some(debris), Some(after)) = (shot("4-debris"), shot("5-apres")) {
            let changed = differing_share(debris, after, 25.0);
            verdict(
                changed > 0.0005,
                format!("la gerbe de débris retombe ({:.3} % des pixels)", changed * 100.0),
            );
        }
        if let Some(after) = shot("5-apres") {
            let m = after.mean_luma(FULL.0, FULL.1);
            info!("luminance moyenne après l'explosion : {m:.2}");
        }
    } else if let (Some(ring), Some(after)) = (shot("2-anneau"), shot("4-apres")) {
        let changed = differing_share(ring, after, 20.0);
        verdict(
            changed > 0.001,
            format!("l'anneau de brume se dessine ({:.2} % des pixels)", changed * 100.0),
        );
    } else {
        verdict(false, "captures de l'anneau absentes".into());
    }
    let peak = check.levels.iter().copied().fold(0.0, f32::max);
    verdict(
        peak <= MAX_FLASH_PEAK + 1e-6,
        format!("éclair au plus {peak:.3} (plafond {MAX_FLASH_PEAK})"),
    );
    let max_step = MAX_FLASH_PEAK / 60.0 / MIN_FADE + 1e-3;
    let worst = check.levels.windows(2).map(|w| (w[0] - w[1]).abs()).fold(0.0, f32::max);
    verdict(
        worst <= max_step,
        format!("aucune variation d'éclair plus rapide que MIN_FADE (pire saut {worst:.4} par image)"),
    );
    if ok {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        error!("vérification : échec");
        AppExit::error()
    }
}
