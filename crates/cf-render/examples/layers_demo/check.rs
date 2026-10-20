//! Le mode `CF_CHECK=1` de `layers_demo` : étapes scriptées, captures d'écran, verdicts sur les pixels.

use std::collections::HashMap;

use bevy::camera::Camera;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::PrimaryWindow;
use cf_render::{CameraPose, Layer, LayerCamera, LayerOpacity};

use crate::{WALL_Z, pose_at};

const WARMUP_FRAMES: u32 = 90;
/// Une étape : on règle l'état, on attend qu'il s'applique, on capture, on attend la capture.
const SETTLE_FRAMES: u32 = 12;
const PHASE_FRAMES: u32 = 22;

/// Une étape de la vérification : pose, opacités (fond, monde, avant-plan, surcouche) et ce qu'on en attend.
struct Phase {
    name: &'static str,
    z: f32,
    opacity: [f32; 4],
}

const PHASES: [Phase; 7] = [
    Phase {
        name: "1-mur-devant-le-logo",
        z: -9.3,
        opacity: [1.0; 4],
    },
    Phase {
        name: "2-camera-dans-le-mur",
        z: WALL_Z,
        opacity: [1.0; 4],
    },
    Phase {
        name: "3-temoin-sans-avant-plan",
        z: -9.3,
        opacity: [1.0, 1.0, 0.0, 1.0],
    },
    Phase {
        name: "4-monde-a-0",
        z: -9.3,
        opacity: [1.0, 0.0, 1.0, 1.0],
    },
    Phase {
        name: "5-monde-a-0,5",
        z: -9.3,
        opacity: [1.0, 0.5, 1.0, 1.0],
    },
    Phase {
        name: "6-surcouche-a-0",
        z: -9.3,
        opacity: [1.0, 1.0, 1.0, 0.0],
    },
    Phase {
        name: "7-fenetre-elargie",
        z: -9.3,
        opacity: [1.0; 4],
    },
];
/// Étape où la fenêtre est élargie, et sa nouvelle taille (largeur seule : l'aspect change, le titre ne doit pas).
const RESIZE_PHASE: usize = 6;
const RESIZED: (u32, u32) = (800, 360);

/// Une capture d'écran : RGBA, ligne par ligne depuis le coin haut gauche.
struct Shot {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Shot {
    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = 4 * (y as usize * self.width as usize + x as usize);
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}

#[derive(Resource, Default)]
pub(crate) struct Check {
    frame: u32,
    shots: HashMap<&'static str, Shot>,
    /// Caméras actives au moment de la capture, par étape : (fond, monde, avant-plan, surcouche).
    active: HashMap<&'static str, [bool; 4]>,
}

pub(crate) fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut pose: ResMut<CameraPose>,
    mut opacity: ResMut<LayerOpacity>,
    cameras: Query<(&LayerCamera, &Camera)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    check.frame += 1;
    let Some(frame) = check.frame.checked_sub(WARMUP_FRAMES) else {
        return;
    };
    let index = (frame / PHASE_FRAMES) as usize;
    let at = frame % PHASE_FRAMES;
    let Some(phase) = PHASES.get(index) else {
        // Les dernières captures arrivent avec une ou deux images de retard.
        if at > 8 {
            exit.write(evaluate(&check));
        }
        return;
    };
    if index == RESIZE_PHASE && at == 0 {
        window.resolution.set(RESIZED.0 as f32, RESIZED.1 as f32);
    }
    *pose = pose_at(phase.z);
    for (layer, value) in Layer::ALL.into_iter().zip(phase.opacity) {
        opacity.set(layer, value);
    }
    if at == SETTLE_FRAMES {
        let mut active = [false; 4];
        for (LayerCamera(layer), camera) in &cameras {
            active[layer.index()] = camera.is_active;
        }
        check.active.insert(phase.name, active);
        let name = phase.name;
        let mut shot = commands.spawn(Screenshot::primary_window());
        shot.observe(move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>| {
            if let Ok(image) = captured.image.clone().try_into_dynamic() {
                let rgba = image.to_rgba8();
                let (width, height) = (rgba.width(), rgba.height());
                check.shots.insert(
                    name,
                    Shot {
                        width,
                        height,
                        rgba: rgba.into_raw(),
                    },
                );
            }
        });
        if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
        }
    }
}

/// Pixel (0 à 1 depuis le coin haut gauche) en `[r, g, b]`.
fn at(image: &Shot, x: f32, y: f32) -> [u8; 3] {
    let px = ((x * image.width as f32) as u32).min(image.width - 1);
    let py = ((y * image.height as f32) as u32).min(image.height - 1);
    image.pixel(px, py)
}

fn is_green([r, g, b]: [u8; 3]) -> bool {
    g > 200 && r < 60 && b < 60
}

fn is_red([r, g, b]: [u8; 3]) -> bool {
    r > 200 && g < 40 && b < 40
}

/// Pixels quasi blancs de la bande du haut, là où se trouve le titre.
fn white_pixels(image: &Shot) -> usize {
    (0..image.height / 4)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| image.pixel(x, y).iter().all(|&c| c > 215))
        .count()
}

/// Largeur (px) du rectangle qui entoure les pixels quasi blancs de la bande du haut : la largeur du titre.
fn title_width(image: &Shot) -> u32 {
    let xs: Vec<u32> = (0..image.height / 4)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| image.pixel(x, y).iter().all(|&c| c > 215))
        .map(|(x, _)| x)
        .collect();
    match (xs.iter().min(), xs.iter().max()) {
        (Some(min), Some(max)) => max - min + 1,
        _ => 0,
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn evaluate(check: &Check) -> AppExit {
    let mut failures = 0;
    let mut verdict = |ok: bool, what: &str| {
        info!("{} {what}", if ok { "OK    " } else { "ÉCHEC " });
        failures += usize::from(!ok);
    };
    let shot = |name: &str| check.shots.iter().find(|(k, _)| k.starts_with(name)).map(|(_, v)| v);
    let active = |name: &str| check.active.iter().find(|(k, _)| k.starts_with(name)).map(|(_, v)| *v);
    let (Some(wall), Some(inside), Some(control), Some(off), Some(half), Some(no_title), Some(wide)) = (
        shot("1-"),
        shot("2-"),
        shot("3-"),
        shot("4-"),
        shot("5-"),
        shot("6-"),
        shot("7-"),
    ) else {
        warn!("captures manquantes : {:?}", check.shots.keys().collect::<Vec<_>>());
        return AppExit::from_code(2);
    };
    let corner = (0.08, 0.5);
    let center = (0.5, 0.5);

    let c = at(wall, center.0, center.1);
    verdict(
        is_green(c),
        &format!("1. mur à 45 cm : le logo d'avant-plan reste visible au centre ({c:?})"),
    );
    let r = at(wall, corner.0, corner.1);
    verdict(
        is_red(r),
        &format!("1. mur à 45 cm : le reste de l'écran est le mur ({r:?})"),
    );
    verdict(
        white_pixels(wall) > 100,
        &format!("1. le titre 2D est dessiné ({} pixels blancs)", white_pixels(wall)),
    );

    let c = at(inside, center.0, center.1);
    verdict(
        is_green(c),
        &format!("2. caméra dans le mur : le logo reste visible ({c:?})"),
    );

    let c = at(control, center.0, center.1);
    verdict(
        is_red(c),
        &format!("3. témoin : un objet du monde à 90 cm est masqué par le mur ({c:?})"),
    );

    let sky = at(off, corner.0, corner.1);
    verdict(
        !is_red(sky) && sky[2] > 20,
        &format!("4. monde à 0 : le mur a disparu, on voit le ciel ({sky:?})"),
    );
    verdict(
        is_green(at(off, center.0, center.1)),
        "4. monde à 0 : le logo reste visible",
    );
    verdict(
        active("4-") == Some([true, false, true, true]),
        &format!(
            "4. monde à 0 : sa caméra est désactivée, les autres actives ({:?})",
            active("4-")
        ),
    );

    // Mélange en lumière linéaire : 0,5 × mur + 0,5 × ciel.
    let mixed = at(half, corner.0, corner.1);
    let wall_px = r;
    let expected: Vec<f32> = (0..3)
        .map(|i| 255.0 * linear_to_srgb(0.5 * srgb_to_linear(wall_px[i]) + 0.5 * srgb_to_linear(sky[i])))
        .collect();
    let worst = (0..3)
        .map(|i| (f32::from(mixed[i]) - expected[i]).abs())
        .fold(0.0, f32::max);
    verdict(
        worst <= 6.0,
        &format!("5. monde à 0,5 : mélange mur/ciel attendu {expected:?}, mesuré {mixed:?} (écart max {worst:.1}/255)"),
    );
    verdict(
        active("5-") == Some([true; 4]),
        "5. monde à 0,5 : la couche rend toujours",
    );

    verdict(
        white_pixels(no_title) == 0,
        &format!(
            "6. surcouche à 0 : plus de titre ({} pixels blancs)",
            white_pixels(no_title)
        ),
    );
    verdict(
        active("6-") == Some([true, true, true, false]),
        &format!("6. surcouche à 0 : sa caméra est désactivée ({:?})", active("6-")),
    );

    let (narrow_width, wide_width) = (title_width(wall), title_width(wide));
    verdict(
        wide.width == RESIZED.0 && wide.height == RESIZED.1,
        &format!(
            "7. la capture a la taille de la fenêtre élargie ({}×{})",
            wide.width, wide.height
        ),
    );
    verdict(
        narrow_width > 100 && narrow_width.abs_diff(wide_width) <= 3,
        &format!("7. les textures suivent la fenêtre : titre de {narrow_width} px avant, {wide_width} px après"),
    );
    verdict(
        is_green(at(wide, center.0, center.1)) && is_red(at(wide, corner.0, corner.1)),
        "7. l'image élargie est complète (logo et mur)",
    );

    if failures == 0 {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        warn!("vérification : {failures} échec(s)");
        AppExit::from_code(1)
    }
}
