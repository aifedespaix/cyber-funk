//! Le mode `CF_CHECK=1` de `postfx_demo` : étapes scriptées, captures, uniformes envoyés, verdicts sur les pixels.
//!
//! L'horloge avance d'exactement 1/60 s par image (`TimeUpdateStrategy::ManualDuration`, posé par `main`) : le rendu
//! logiciel tourne à quelques images par seconde, et le limiteur borne son pas à 1/30 s ; sans cela les fondus du
//! limiteur et la course de la vague se décaleraient. Une étape capture dès que sa condition est remplie (durée
//! écoulée, flash au sommet, vague au milieu de l'écran) ; seul le préchauffage se compte en temps réel.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use cf_render::postfx::GradeUniforms;
use cf_render::{AudioFeatures, Compositor, CurrentShowFx, PostFxSettings, ShowFxInput};

use crate::{CALM, DROP, SIZE, TITLE_GREY};

/// Charte de couleurs de la scène (linéaire) : rouge, vert, bleu, cyan, blanc, gris 18 %.
pub(crate) const CHART: [(f32, f32, f32); 6] = [
    (0.8, 0.05, 0.05),
    (0.05, 0.6, 0.1),
    (0.05, 0.1, 0.8),
    (0.0, 0.6, 0.8),
    (0.8, 0.8, 0.8),
    (0.18, 0.18, 0.18),
];

/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 6.0;
/// Temps (s, horloge de l'image) laissé à la capture pour arriver avant l'étape suivante.
const AFTER_CAPTURE_S: f64 = 0.3;
/// Au-delà, l'étape échoue (sa condition n'est jamais venue).
const PHASE_TIMEOUT_S: f64 = 10.0;

/// Quand capturer.
#[derive(Clone, Copy)]
enum When {
    /// Après tant de secondes dans l'étape.
    After(f64),
    /// Dès que le flash (sortie du limiteur) dépasse ce niveau.
    FlashAbove(f32),
    /// Quand la vague passe au milieu de l'écran.
    SweepMid,
}

struct Phase {
    name: &'static str,
    show: Option<ShowFxInput>,
    pulse: f32,
    bloom: bool,
    /// Un drop frappe à l'entrée de l'étape (une image).
    drop: bool,
    /// Le cue change à l'entrée de l'étape.
    next_cue: bool,
    when: When,
}

const fn phase(name: &'static str, show: Option<ShowFxInput>, when: When) -> Phase {
    Phase {
        name,
        show,
        pulse: 0.0,
        bloom: true,
        drop: false,
        next_cue: false,
        when,
    }
}

const PHASES: [Phase; 7] = [
    phase("1-neutre", None, When::After(0.5)),
    Phase {
        pulse: 1.0,
        ..phase("2-kick", None, When::After(0.5))
    },
    // Constante de temps de l'étalonnage : 0,83 s ; à 4 s, 99 % du chemin.
    phase("3-chaleur", Some(ShowFxInput { heat: 1.0, ..CALM }), When::After(4.0)),
    Phase {
        drop: true,
        ..phase("4-flash-de-drop", Some(CALM), When::FlashAbove(0.3))
    },
    Phase {
        next_cue: true,
        ..phase("5-vague", Some(CALM), When::SweepMid)
    },
    phase(
        "6-bullet-time",
        Some(ShowFxInput {
            time_scale: 0.2,
            ..CALM
        }),
        When::After(1.0),
    ),
    Phase {
        bloom: false,
        ..phase("7-sans-bloom", None, When::After(0.5))
    },
];

/// Une capture d'écran : RGBA sRGB 8 bits, ligne par ligne depuis le coin haut gauche.
struct Shot {
    width: u32,
    rgba: Vec<u8>,
}

impl Shot {
    fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        let i = 4 * (y as usize * self.width as usize + x as usize);
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]].map(f32::from)
    }

    /// Moyenne d'un rectangle `[x0, x1) × [y0, y1)`.
    fn mean(&self, (x0, y0): (u32, u32), (x1, y1): (u32, u32)) -> [f32; 3] {
        let mut sum = [0.0; 3];
        let mut n = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                let p = self.pixel(x, y);
                for c in 0..3 {
                    sum[c] += p[c];
                }
                n += 1.0;
            }
        }
        sum.map(|s| s / n)
    }
}

fn luma(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

fn saturation(p: [f32; 3]) -> f32 {
    p.iter().copied().fold(0.0, f32::max) - p.iter().copied().fold(255.0, f32::min)
}

#[derive(Resource, Default)]
pub(crate) struct Check {
    start: Option<Instant>,
    index: usize,
    entered: Option<f64>,
    captured: Option<f64>,
    cue_seq: u32,
    shots: HashMap<&'static str, Shot>,
    uniforms: Vec<(&'static str, GradeUniforms, f32)>,
    failed: Vec<&'static str>,
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    time: Res<Time<Real>>,
    mut audio: ResMut<AudioFeatures>,
    mut settings: ResMut<PostFxSettings>,
    state: Res<cf_render::PostFxState>,
    compositor: Single<(&GradeUniforms, Option<&Bloom>), With<Compositor>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    if start.elapsed().as_secs_f64() < WARMUP_S {
        return;
    }
    let now = time.elapsed_secs_f64();
    let Some(phase) = PHASES.get(check.index) else {
        exit.write(evaluate(&check));
        return;
    };
    let entered = match check.entered {
        Some(t) => t,
        None => {
            // Entrée dans l'étape : on règle le spectacle, l'audio et la qualité.
            check.entered = Some(now);
            if phase.next_cue {
                check.cue_seq += 1;
            }
            match phase.show {
                Some(show) => commands.insert_resource(CurrentShowFx(ShowFxInput {
                    cue_seq: check.cue_seq,
                    drop: phase.drop.then_some(DROP),
                    ..show
                })),
                None => commands.remove_resource::<CurrentShowFx>(),
            }
            audio.0.pulse = phase.pulse;
            audio.0.section = cf_analysis::Section::Peak;
            settings.bloom = phase.bloom;
            return;
        }
    };
    // Le drop ne frappe que pendant une image (`drop_now`).
    if phase.drop && let Some(show) = phase.show {
        commands.insert_resource(CurrentShowFx(ShowFxInput {
            cue_seq: check.cue_seq,
            ..show
        }));
    }
    if let Some(captured) = check.captured {
        if now - captured > AFTER_CAPTURE_S {
            check.index += 1;
            check.entered = None;
            check.captured = None;
        }
        return;
    }
    let fx = state.show_fx();
    let ready = match phase.when {
        When::After(s) => now - entered >= s,
        When::FlashAbove(level) => fx.flash() >= level,
        When::SweepMid => fx.sweep() > 0.05 && (0.45..0.65).contains(&fx.sweep_pos()),
    };
    if !ready {
        if now - entered > PHASE_TIMEOUT_S {
            error!("{} : condition de capture jamais remplie", phase.name);
            check.failed.push(phase.name);
            check.captured = Some(now);
        }
        return;
    }
    check.captured = Some(now);
    let (grade, bloom) = *compositor;
    check
        .uniforms
        .push((phase.name, *grade, bloom.map_or(0.0, |b| b.intensity)));
    let name = phase.name;
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
    });
    if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
        let _ = std::fs::write(dir.join("phases.json"), phases_json(&check.uniforms));
    }
}

/// Les uniformes de chaque étape, pour rendre la référence Sillage avec exactement les mêmes valeurs.
fn phases_json(rows: &[(&'static str, GradeUniforms, f32)]) -> String {
    let mut out = String::from("[\n");
    for (i, (name, g, bloom)) in rows.iter().enumerate() {
        let _ = write!(
            out,
            "  {{\"name\": \"{name}\", \"bloom\": {bloom}, \"aberration\": {}, \"time\": {}, \"grain\": {}, \
             \"lens\": {}, \"rainTime\": {}, \"aspect\": {}, \"flash\": {}, \"sweep\": {}, \"sweepPos\": {}, \
             \"contrast\": {}, \"saturation\": {}, \"slow\": {}, \"blur\": {}, \"flashColor\": [{}, {}, {}], \
             \"sweepColor\": [{}, {}, {}]}}{}\n",
            g.aberration,
            g.time,
            g.grain,
            g.lens,
            g.rain_time,
            g.aspect,
            g.flash,
            g.sweep,
            g.sweep_pos,
            g.contrast,
            g.saturation,
            g.slow,
            g.blur,
            g.flash_color.x,
            g.flash_color.y,
            g.flash_color.z,
            g.sweep_color.x,
            g.sweep_color.y,
            g.sweep_color.z,
            if i + 1 < rows.len() { "," } else { "" },
        );
    }
    out.push(']');
    out
}

/// Zones du titre et du nom du DJ (px) : seuls endroits où la surcouche dessine.
const TITLE_ZONES: [((u32, u32), (u32, u32)); 2] = [((200, 100), (760, 200)), ((20, 460), (330, 540))];
/// Ciel au-dessus de l'horizon, hors titre.
const SKY: ((u32, u32), (u32, u32)) = ((465, 210), (495, 232));
/// Pastille rouge de la charte.
const RED_PATCH: ((u32, u32), (u32, u32)) = ((320, 262), (340, 278));
/// Juste sous l'arête néon de la première boîte de gauche : du halo, pas de néon.
const NEAR_NEON: ((u32, u32), (u32, u32)) = ((268, 33), (300, 42));

fn evaluate(check: &Check) -> AppExit {
    let mut ok = check.failed.is_empty();
    let mut verdict = |pass: bool, what: String| {
        if pass {
            info!("OK     {what}");
        } else {
            error!("ÉCHEC  {what}");
            ok = false;
        }
    };
    let shot = |name: &str| check.shots.get(name);
    let Some(neutral) = shot("1-neutre") else {
        error!("capture neutre absente");
        return AppExit::error();
    };
    let width = neutral.width;
    let height = SIZE.1;

    // La surcouche : les pixels pleinement couverts par le titre dans la capture neutre, à sa couleur exacte.
    let grey = (TITLE_GREY.powf(1.0 / 2.4) * 1.055 - 0.055) * 255.0;
    let mut mask = Vec::new();
    for ((x0, y0), (x1, y1)) in TITLE_ZONES {
        for y in y0..y1.min(height) {
            for x in x0..x1.min(width) {
                if neutral.pixel(x, y).iter().all(|c| (c - grey).abs() <= 2.0) {
                    mask.push((x, y));
                }
            }
        }
    }
    verdict(mask.len() > 800, format!("le titre 2D est dessiné ({} pixels à sa couleur)", mask.len()));
    for phase in &PHASES[1..] {
        let Some(image) = shot(phase.name) else {
            verdict(false, format!("{} : capture absente", phase.name));
            continue;
        };
        // Pixel pour pixel, la même valeur que dans la capture neutre.
        let worst = mask
            .iter()
            .map(|&(x, y)| {
                let (p, n) = (image.pixel(x, y), neutral.pixel(x, y));
                (0..3).map(|c| (p[c] - n[c]).abs()).fold(0.0, f32::max)
            })
            .fold(0.0, f32::max);
        verdict(
            worst < 0.5,
            format!("{} : le titre n'a subi aucun effet (écart max {worst}/255)", phase.name),
        );
    }

    let sky = luma(neutral.mean(SKY.0, SKY.1));
    let red = saturation(neutral.mean(RED_PATCH.0, RED_PATCH.1));
    let halo = luma(neutral.mean(NEAR_NEON.0, NEAR_NEON.1));
    if let Some(image) = shot("2-kick") {
        let k = luma(image.mean(NEAR_NEON.0, NEAR_NEON.1));
        verdict(k > halo + 1.0, format!("kick : le bloom respire ({halo:.1} → {k:.1})"));
    }
    if let Some(image) = shot("3-chaleur") {
        let r = saturation(image.mean(RED_PATCH.0, RED_PATCH.1));
        verdict(r > red + 2.0, format!("chaleur : la charte sature davantage ({red:.1} → {r:.1})"));
    }
    if let Some(image) = shot("4-flash-de-drop") {
        let s = luma(image.mean(SKY.0, SKY.1));
        verdict(s > sky + 20.0, format!("flash de drop : le ciel s'éclaire ({sky:.1} → {s:.1})"));
    }
    if let Some((image, row)) = shot("5-vague").zip(check.uniforms.iter().find(|r| r.0 == "5-vague")) {
        let x = ((row.1.sweep_pos * width as f32) as u32).clamp(10, width - 10);
        let s = luma(image.mean((x - 8, 210), (x + 8, 232)));
        let n = luma(neutral.mean((x - 8, 210), (x + 8, 232)));
        verdict(s > n + 5.0, format!("vague : la bande passe à x = {x} px ({n:.1} → {s:.1})"));
    }
    if let Some(image) = shot("6-bullet-time") {
        let r = saturation(image.mean(RED_PATCH.0, RED_PATCH.1));
        verdict(r < red - 5.0, format!("bullet-time : l'image se désature ({red:.1} → {r:.1})"));
    }
    if let Some(image) = shot("7-sans-bloom") {
        let h = luma(image.mean(NEAR_NEON.0, NEAR_NEON.1));
        verdict(h < halo - 1.0, format!("sans bloom : plus de halo près du néon ({halo:.1} → {h:.1})"));
    }
    if ok {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        error!("vérification : échec");
        AppExit::error()
    }
}
