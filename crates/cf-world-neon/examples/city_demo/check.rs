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
use cf_render::audio::{FilteredSignals, Target, VisualDensity};
use cf_render::postfx::measure::{PaletteHues, hue_degrees, measure};
use cf_render::{AbsolutePosition, AudioFeatures, DiagnosticsSnapshot, FloatingOrigin, SceneColor, ScenePalette};
use cf_show::density::DensityState;
use cf_world_neon::{FacadeLook, RoadLook, Runner};

use crate::SIZE;
use crate::scenes::{ForcedView, SCENES};
use crate::shots::{Shot, differing_share, luma, worst_diff};

/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 10.0;
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
    /// Teintes des rôles de la palette (dominante : le cyan des tours, accent : le corail), lues à la première scène.
    hues: Option<PaletteHues>,
}

/// `CF_SCENES=6-,7-` : ne joue que les scènes dont le nom contient l'un des fragments (itérer vite sur une capture).
fn wanted(name: &str) -> bool {
    std::env::var("CF_SCENES").map_or(true, |list| list.split(',').any(|part| name.contains(part.trim())))
}

/// Teintes des rôles de la palette : dominante, le cyan des tours ; accent, le corail.
fn palette_hues(palette: &ScenePalette) -> PaletteHues {
    let hue = |c: SceneColor| {
        let c = palette.get(c);
        hue_degrees([c.r, c.g, c.b]).unwrap_or(0.0)
    };
    PaletteHues {
        dominant: hue(SceneColor::Mint),
        accent: hue(SceneColor::Rose),
    }
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut audio: ResMut<AudioFeatures>,
    mut road: ResMut<RoadLook>,
    mut facade: ResMut<FacadeLook>,
    origin: Res<FloatingOrigin>,
    palette: Res<ScenePalette>,
    snapshot: Res<DiagnosticsSnapshot>,
    mut runner: Query<(&mut AbsolutePosition, &mut Visibility), With<Runner>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    check.hues.get_or_insert_with(|| palette_hues(&palette));
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    while SCENES.get(check.index).is_some_and(|s| !wanted(s.name)) {
        check.index += 1;
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
            audio.0.intensity = scene.intensity;
            commands.insert_resource(VisualDensity(DensityState {
                delta: scene.density,
                ..DensityState::NEUTRAL
            }));
            commands.insert_resource(ForcedView(scene.view));
            let mut signals = FilteredSignals::default().with(Target::Puddles, scene.puddles);
            // Un spectre en pente (graves forts, aigus faibles), pour les façades égaliseur (C4).
            for (i, band) in signals.spectrum.iter_mut().enumerate() {
                *band = scene.intensity * (1.0 - 0.6 * i as f32 / 31.0);
            }
            if let Some(age) = scene.kick_age {
                signals.kick_wave.push();
                signals.kick_wave.age(age);
            }
            commands.insert_resource(signals);
            road.wet = scene.wet;
            road.rain = scene.rain;
            facade.build = scene.build;
            facade.clock_s = Some(scene.time_s);
            road.clock_s = Some(scene.time_s);
            check.stage = Stage::Settle(0);
        }
        Stage::Settle(frames) if frames < scene.settle => check.stage = Stage::Settle(frames + 1),
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
    recenter_verdicts(check, &mut verdict);
    measure_verdicts(check, &mut verdict);
    if ok {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        error!("vérification : échec");
        AppExit::error()
    }
}

/// Recentrage de l'origine flottante : la même ville de part et d'autre.
fn recenter_verdicts(check: &Check, verdict: &mut impl FnMut(bool, String)) {
    let shot = |name: &str| check.shots.get(name);
    if let (Some(a), Some(b)) = (shot("6-seuil-direct"), shot("7-seuil-via-km30")) {
        // Les faisceaux, les planètes et les étoiles sont animés par l'horloge : ils diffèrent d'une capture à l'autre quelle
        // que soit l'origine. Le verdict porte donc sur ce qui ne dépend que de la position : le sol (chaussée, brume
        // basse, nappe) et les façades des deux bords de l'image, que rien d'animé ne traverse.
        let share_in = |(x0, y0): (u32, u32), (x1, y1): (u32, u32)| {
            let mut n = 0_u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let (p, q) = (a.pixel(x, y), b.pixel(x, y));
                    n += u32::from((0..3).any(|c| (p[c] - q[c]).abs() > 3.0));
                }
            }
            n as f32 / ((x1 - x0) * (y1 - y0)) as f32
        };
        let ground = share_in((0, 405), (SIZE.0, SIZE.1));
        verdict(
            ground < 0.002,
            format!(
                "recentrage d'origine : même sol ({:.3} % des pixels à plus de 3/255)",
                ground * 100.0
            ),
        );
        let facades = share_in((0, 0), (160, 225)).max(share_in((800, 45), (SIZE.0, 225)));
        verdict(
            facades < 0.02,
            format!(
                "recentrage d'origine : mêmes façades ({:.3} % des pixels à plus de 3/255)",
                facades * 100.0
            ),
        );
        let (worst, share) = (worst_diff(a, b), differing_share(a, b, 3.0));
        info!(
            "recentrage d'origine : image entière, {:.1} % des pixels diffèrent (faisceaux et ciel animés), écart max {worst}/255",
            share * 100.0
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
}

/// Mesures de palette et de luminance sur les scènes d'atmosphère et de surface.
fn measure_verdicts(check: &Check, verdict: &mut impl FnMut(bool, String)) {
    let shot = |name: &str| check.shots.get(name);
    // Mesures de DA §1.1 et §3.1 sur les scènes d'atmosphère et de surface (rôles de la palette, pas les pics de
    // l'histogramme) : `Ȳ`, part d'émissifs, part des émissifs dans la dominante ou l'accent, part de glace.
    if let Some(hues) = check.hues {
        for name in ["8-atmo-calme", "9-atmo-sommet", "11-mouille-kick", "12-fenetres-face"] {
            if let Some(s) = shot(name) {
                let r = measure(&s.rgba, s.width, hues, None);
                info!(
                    "mesure {name} : Ȳ {:.3}, émissifs {:.1} %, dans la palette {:.0} %, glace {:.2} %",
                    r.mean_y,
                    r.emissive_share * 100.0,
                    r.emissive_in_palette * 100.0,
                    r.ice_share * 100.0
                );
                if name != "12-fenetres-face" {
                    verdict(
                        r.ice_share < 0.02,
                        format!("{name} : moins de 2 % de glace ({:.2} %)", r.ice_share * 100.0),
                    );
                }
            }
        }
    }
}
