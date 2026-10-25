//! Le mode `CF_CHECK=1` de `traffic_demo` : laisse le rendu logiciel compiler ses pipelines, attend que la vitrine et le
//! trafic de fond vivent, capture l'écran et vérifie sur les pixels que le trafic est dessiné.

use std::time::Instant;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_world_neon::car::CarRig;
use cf_world_neon::traffic::TrafficStats;

use crate::SIZE;

/// Temps réel (s) laissé au démarrage : décodage du modèle de la voiture et compilation des pipelines (rendu logiciel).
const WARMUP_S: f64 = 25.0;
/// Images de scène laissées au trafic avant la capture : quatre secondes à 1/20 s, de quoi remplir les couloirs et
/// dessiner les murs de lumière.
const SETTLE_FRAMES: u32 = 100;

/// `CF_EVENT=<slug>` : un événement du trafic jugé seul ; la vérification se réduit alors à « la scène est dessinée ».
fn single_event() -> Option<String> {
    std::env::var("CF_EVENT").ok()
}

/// Images de scène avant la capture : `CF_FRAMES`, sinon [`SETTLE_FRAMES`].
fn settle_frames() -> u32 {
    std::env::var("CF_FRAMES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(SETTLE_FRAMES)
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Stage {
    #[default]
    Warm,
    Settle(u32),
    Await,
}

#[derive(Resource, Default)]
pub(crate) struct Check {
    start: Option<Instant>,
    stage: Stage,
    /// Pixels lumineux de la capture, et nombre de pixels saturés de teinte « police » (rouge ou bleu vif).
    shot: Option<(f32, f32)>,
}

fn screenshot(commands: &mut Commands) {
    let mut shot = commands.spawn(Screenshot::primary_window());
    shot.observe(|captured: On<ScreenshotCaptured>, mut check: ResMut<Check>| {
        let counts = captured.image.clone().try_into_dynamic().map_or((0.0, 0.0), |image| {
            let rgba = image.to_rgba8();
            let bright = rgba
                .pixels()
                .filter(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) > 90.0)
                .count();
            // Un gyrophare allumé ou un point rouge/bleu vif : dominante nette d'un canal.
            let vivid = rgba
                .pixels()
                .filter(|p| {
                    let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
                    (r > 200 && g < 120 && b < 120) || (b > 200 && r < 120 && g < 160)
                })
                .count();
            (bright as f32, vivid as f32)
        });
        check.shot = Some(counts);
    });
    if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        let name = single_event().map_or_else(|| "trafic".to_owned(), |slug| format!("trafic-{slug}"));
        shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
    }
}

/// `CfSet::Present` : la machine à états de la vérification.
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    stats: Res<TrafficStats>,
    director: Res<cf_world_neon::traffic::TrafficDirector>,
    car: Query<&cf_render::AbsolutePosition, With<cf_render::StreamFocus>>,
    rigs: Query<(), With<CarRig>>,
    mut exit: MessageWriter<AppExit>,
) {
    if rigs.is_empty() {
        // La voiture n'est pas encore posée : le temps de chargement ne compte pas.
        check.start = Some(Instant::now());
        return;
    }
    let start = *check.start.get_or_insert_with(Instant::now);
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    match check.stage {
        Stage::Warm => check.stage = Stage::Settle(0),
        Stage::Settle(n) if n < settle_frames() => check.stage = Stage::Settle(n + 1),
        Stage::Settle(_) => {
            info!("trafic avant la capture : {:?}", *stats);
            if let Ok(car) = car.single() {
                director.collect_subjects(|s| {
                    if s.kind == cf_render::SubjectKind::Landmark {
                        info!(
                            "sujet de structure : {:?} m devant la voiture, taille {}",
                            car.0.z - s.position.z,
                            s.size
                        );
                    }
                });
            }
            screenshot(&mut commands);
            check.stage = Stage::Await;
        }
        Stage::Await => {
            let Some((bright, vivid)) = check.shot else { return };
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
                bright > 3000.0,
                format!(
                    "capture de {}×{} px : la scène est dessinée ({bright} pixels lumineux)",
                    SIZE.0, SIZE.1
                ),
            );
            if let Some(slug) = single_event() {
                verdict(
                    stats.events >= 1,
                    format!("l'événement « {slug} » vit ({} en cours)", stats.events),
                );
                exit.write(if ok { AppExit::Success } else { AppExit::error() });
                return;
            }
            verdict(
                vivid > 20.0,
                format!("des éclats rouges ou bleus vifs (gyrophares, points) sont dessinés ({vivid} pixels)"),
            );
            verdict(
                stats.fleet.active >= 6,
                format!("{} véhicules vivants", stats.fleet.active),
            );
            verdict(stats.glints >= 2, format!("{} points lumineux posés", stats.glints));
            verdict(stats.walls >= 1, format!("{} murs de lumière actifs", stats.walls));
            exit.write(if ok { AppExit::Success } else { AppExit::error() });
        }
    }
}
