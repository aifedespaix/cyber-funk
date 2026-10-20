//! Les palettes dans une `App` sans GPU : le thème du show (passation de DJ, une mesure), les demandes de l'opérateur
//! (temps fort), la teinte du look et l'ambiance. Le temps de l'image est manuel (60 i/s).

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::palette::{PaletteChanged, PaletteRequest, PaletteTiming, PaletteWave};
use cf_render::{
    AudioFeatures, CorePlugin, CurrentShow, PaletteDriver, PalettePlugin, SceneColor, SceneMood, ScenePalette,
    ThemeLibrary,
};
use cf_show::director::ShowDirector;
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_show::model::{PresetTheme, Theme};
use rand_pcg::Pcg32;

const FRAME: Duration = Duration::from_micros(16_667);

/// Un mesure à 128 bpm, comme la calcule le chef du spectacle pour une passation.
const BAR_S: f32 = 60.0 / 128.0 * 4.0;

fn demo_show() -> CurrentShow {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = LookLibrary::load_dir(&assets.join("looks")).unwrap();
    let show = LoadedShow::load(&assets.join("shows/demo.ron"), &looks).unwrap();
    let director = ShowDirector::<Pcg32>::new(&show, DropTitles::empty(), 1).unwrap();
    CurrentShow(director.state().clone())
}

fn app(with_show: bool) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin, PalettePlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    if with_show {
        app.insert_resource(demo_show());
    }
    app.update();
    app.update();
    app
}

fn library() -> ThemeLibrary {
    ThemeLibrary::builtin().unwrap()
}

fn bytes(app: &App, role: SceneColor) -> [u8; 3] {
    app.world().resource::<ScenePalette>().get(role).to_srgb8()
}

fn spread(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0)
}

fn set_show_theme(app: &mut App, theme: Theme, fade_s: f32) {
    let mut show = app.world_mut().resource_mut::<CurrentShow>();
    show.0.theme = Arc::new(theme);
    show.0.theme_seq += 1;
    show.0.theme_fade_s = fade_s;
}

#[test]
fn le_theme_du_show_est_pose_sans_fondu_a_l_ouverture() {
    let app = app(true);
    // Le créneau 0 de la démo est « Velvet » ; la première lecture ne glisse pas depuis un autre thème.
    let show_theme = app.world().resource::<CurrentShow>().theme.clone();
    let expected = library().resolve(&show_theme);
    assert_eq!(app.world().resource::<ScenePalette>(), &expected);
    assert!(!app.world().resource::<PaletteDriver>().busy());
}

#[test]
fn changement_de_dj_fondu_sur_une_mesure_sans_saut() {
    let mut app = app(true);
    let before = *app.world().resource::<ScenePalette>();
    let target = library().resolve(&Theme::Preset(PresetTheme::Frost));
    set_show_theme(&mut app, Theme::Preset(PresetTheme::Frost), BAR_S);

    let mut previous = bytes(&app, SceneColor::Mint);
    let (mut frames, mut worst) = (0, 0);
    while frames < 400 {
        app.update();
        frames += 1;
        let now = bytes(&app, SceneColor::Mint);
        worst = worst.max(spread(previous, now));
        previous = now;
        if !app.world().resource::<PaletteDriver>().busy() {
            break;
        }
    }
    // Une mesure à 60 i/s, à une ou deux images près.
    let expected = (BAR_S * 60.0) as i32;
    assert!((frames - expected).abs() <= 3, "{frames} images, attendu ~{expected}");
    assert!(worst <= 12, "saut de {worst} octets en une image");
    assert_ne!(*app.world().resource::<ScenePalette>(), before);
    assert_eq!(
        app.world().resource::<ScenePalette>(),
        &target,
        "arrive exactement sur la cible"
    );
}

#[test]
fn la_vague_part_une_fois_a_mi_fondu_et_la_fin_est_signalee() {
    let mut app = app(true);
    set_show_theme(&mut app, Theme::Preset(PresetTheme::Gold), BAR_S);
    let (mut waves, mut settled, mut changed) = (0, 0, 0);
    // La pose sans fondu de l'ouverture a laissé son `settled` dans la file : on la vide.
    app.world_mut().resource_mut::<Messages<PaletteChanged>>().clear();
    for _ in 0..180 {
        app.update();
        waves += app.world_mut().resource_mut::<Messages<PaletteWave>>().drain().count();
        for event in app.world_mut().resource_mut::<Messages<PaletteChanged>>().drain() {
            if event.settled {
                settled += 1;
            } else {
                changed += 1;
            }
        }
    }
    assert_eq!(waves, 1);
    assert_eq!(settled, 1);
    assert!(changed >= 3, "{changed}");
}

#[test]
fn une_demande_de_l_operateur_attend_un_temps_fort_puis_dure_seize_temps() {
    let mut app = app(false);
    let start = *app.world().resource::<ScenePalette>();
    app.world_mut().write_message(PaletteRequest {
        theme: Theme::Preset(PresetTheme::Terminal),
        timing: PaletteTiming::Downbeat,
    });
    // Pas de kick : le fondu ne part qu'après 4 s d'attente au plus, puis dure 8 s (tempo inconnu).
    for _ in 0..200 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<ScenePalette>(),
        &start,
        "toujours en attente du temps fort"
    );
    for _ in 0..(8 * 60 + 120) {
        app.update();
    }
    let target = library().resolve(&Theme::Preset(PresetTheme::Terminal));
    assert_eq!(app.world().resource::<ScenePalette>(), &target);
}

#[test]
fn le_cinema_entre_tout_de_suite_sans_vague() {
    let mut app = app(false);
    app.world_mut().write_message(PaletteRequest {
        theme: Theme::Preset(PresetTheme::Frost),
        timing: PaletteTiming::Now,
    });
    let mut waves = 0;
    for _ in 0..(8 * 60 + 30) {
        app.update();
        waves += app.world_mut().resource_mut::<Messages<PaletteWave>>().drain().count();
    }
    assert_eq!(waves, 0);
    let target = library().resolve(&Theme::Preset(PresetTheme::Frost));
    assert_eq!(app.world().resource::<ScenePalette>(), &target);
}

#[test]
fn la_teinte_du_look_tourne_la_palette_sans_la_perdre() {
    let mut app = app(true);
    let base = *app.world().resource::<ScenePalette>();
    app.world_mut().resource_mut::<CurrentShow>().0.hue_shift_deg = 60.0;
    app.update();
    let rotated = *app.world().resource::<ScenePalette>();
    assert_ne!(rotated, base);
    app.world_mut().resource_mut::<CurrentShow>().0.hue_shift_deg = 0.0;
    app.update();
    assert_eq!(
        app.world().resource::<ScenePalette>(),
        &base,
        "la rotation ne s'accumule pas"
    );
}

#[test]
fn l_ambiance_suit_l_intensite_et_la_palette() {
    let mut app = app(false);
    app.world_mut().insert_resource(AudioFeatures::default());
    let intensity_at = |app: &mut App, value: f32| {
        let mut features = cf_analysis::AudioFeatures::NEUTRAL;
        features.intensity = value;
        app.world_mut().insert_resource(AudioFeatures(features));
        app.update();
        app.world().resource::<SceneMood>().color.to_srgb8()
    };
    let calm = intensity_at(&mut app, 0.0);
    let peak = intensity_at(&mut app, 1.0);
    let palette = *app.world().resource::<ScenePalette>();
    assert_eq!(calm, palette.get(SceneColor::Azure).to_srgb8());
    assert_eq!(peak, palette.get(SceneColor::Rose).to_srgb8());
}
