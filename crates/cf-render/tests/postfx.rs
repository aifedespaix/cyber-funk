//! Le post-process sans GPU : une `App` avec `MinimalPlugins`, le rig de couches et le post-process.
//!
//! Ce que ces tests tiennent : qui porte quoi (le compositeur est équipé, le compositeur de la surcouche ne l'est
//! jamais), le flash qui n'atteint les uniformes que par le limiteur, la bascule du bloom par la qualité. Le rendu
//! lui-même (shader, ordre bloom → étalonnage → tonemapping, surcouche intacte) est vérifié par
//! `examples/postfx_demo` sous Xvfb.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::Camera;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::postfx::{BLOOM_STRENGTH, GradeUniforms};
use cf_render::{
    Compositor, CorePlugin, CurrentShowFx, FloatingOrigin, Layer, LayerOpacity, LayersPlugin, OverlayCompositor,
    PostFxPlugin, PostFxSettings, ShowFxInput,
};
use cf_show::drop::DropFxPackage;
use cf_show::limiter::{MAX_FLASH_PEAK, is_saturated_red};

const FRAME: Duration = Duration::from_micros(16_667);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), CorePlugin))
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_resource::<FloatingOrigin>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .add_plugins((
            LayersPlugin {
                fallback_size: UVec2::new(64, 36),
            },
            PostFxPlugin,
        ));
    app.update();
    app.update();
    app
}

fn compositor(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<Compositor>>()
        .single(app.world())
        .expect("un compositeur")
}

fn overlay_compositor(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<OverlayCompositor>>()
        .single(app.world())
        .expect("un compositeur de surcouche")
}

fn grade(app: &mut App) -> GradeUniforms {
    let entity = compositor(app);
    *app.world()
        .get::<GradeUniforms>(entity)
        .expect("uniformes d'étalonnage")
}

fn show() -> ShowFxInput {
    ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    }
}

#[test]
fn le_compositeur_porte_bloom_etalonnage_et_tonemapping_neutre() {
    let mut app = app();
    let entity = compositor(&mut app);
    let world = app.world();
    assert!(world.get::<GradeUniforms>(entity).is_some());
    assert_eq!(world.get::<Tonemapping>(entity), Some(&Tonemapping::KhronosPbrNeutral));
    let bloom = world.get::<Bloom>(entity).expect("bloom");
    assert!(bloom.intensity >= BLOOM_STRENGTH * 0.5);
}

#[test]
fn la_surcouche_echappe_a_tout_effet() {
    let mut app = app();
    let entity = overlay_compositor(&mut app);
    let main = compositor(&mut app);
    let world = app.world();
    assert!(world.get::<GradeUniforms>(entity).is_none());
    assert!(world.get::<Bloom>(entity).is_none());
    assert_eq!(world.get::<Tonemapping>(entity), Some(&Tonemapping::None));
    let camera = world.get::<Camera>(entity).expect("Camera");
    assert!(matches!(camera.clear_color, ClearColorConfig::None));
    // Rendu après le compositeur, donc après son post-process.
    let main_order = world.get::<Camera>(main).expect("Camera").order;
    assert!(camera.order > main_order);
}

#[test]
fn le_compositeur_de_surcouche_s_eteint_avec_elle() {
    let mut app = app();
    let entity = overlay_compositor(&mut app);
    assert!(app.world().get::<Camera>(entity).expect("Camera").is_active);
    app.world_mut().resource_mut::<LayerOpacity>().set(Layer::Overlay, 0.0);
    app.update();
    assert!(!app.world().get::<Camera>(entity).expect("Camera").is_active);
    app.world_mut().resource_mut::<LayerOpacity>().set(Layer::Overlay, 0.5);
    app.update();
    assert!(app.world().get::<Camera>(entity).expect("Camera").is_active);
}

#[test]
fn sans_spectacle_aucun_flash_n_atteint_les_uniformes() {
    let mut app = app();
    for _ in 0..120 {
        app.update();
        let g = grade(&mut app);
        assert!(g.flash.abs() < f32::EPSILON);
        assert!(g.sweep.abs() < f32::EPSILON);
    }
}

#[test]
fn le_flash_du_drop_n_arrive_aux_uniformes_que_borne_par_le_limiteur() {
    let mut app = app();
    app.insert_resource(CurrentShowFx(show()));
    app.update();
    // Le drop demande un flash plein, plusieurs images de suite (un pont mal écrit) : le limiteur tient.
    let hit = ShowFxInput {
        drop: Some(DropFxPackage {
            flash: 1.0,
            rgb_split: 1.0,
            city_pulse: 0.0,
            shockwaves: 1,
            burst: 20,
        }),
        ..show()
    };
    app.insert_resource(CurrentShowFx(hit));
    let mut top = 0.0_f32;
    let mut split = 0.0_f32;
    for _ in 0..120 {
        app.update();
        let g = grade(&mut app);
        top = top.max(g.flash);
        split = split.max(g.aberration);
        assert!(g.flash <= MAX_FLASH_PEAK + 1e-6);
        let color = cf_show::limiter::Rgb::new(g.flash_color.x, g.flash_color.y, g.flash_color.z);
        assert!(!is_saturated_red(color));
    }
    assert!(top > 0.0, "le drop a bien flashé");
    // La séparation RVB du drop (`rgb_split × 0,014`) est passée dans l'aberration.
    assert!(split > 0.01, "{split}");
}

#[test]
fn la_qualite_eteint_et_rallume_le_bloom() {
    let mut app = app();
    let entity = compositor(&mut app);
    app.world_mut().resource_mut::<PostFxSettings>().bloom = false;
    app.update();
    assert!(app.world().get::<Bloom>(entity).is_none());
    {
        let mut settings = app.world_mut().resource_mut::<PostFxSettings>();
        settings.bloom = true;
        settings.bloom_divisor = 4;
    }
    app.update();
    let bloom = app.world().get::<Bloom>(entity).expect("bloom rallumé");
    // Pas de fenêtre : textures à 64×36, premier niveau borné à 64 px.
    assert_eq!(bloom.max_mip_dimension, 64);
}

#[test]
fn le_rapport_d_aspect_suit_les_textures() {
    let mut app = app();
    let g = grade(&mut app);
    assert!((g.aspect - 64.0 / 36.0).abs() < 1e-4, "{}", g.aspect);
    assert!(g.time >= 0.0 && g.time < 1.0);
}
