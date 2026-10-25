//! Les cascades dans une vraie `App` Bevy, sans rendu : le pilote conduit, le directeur lance une cascade (registre limité
//! à un faux script, indépendant des vrais de J7-07), fond sa pose dans l'abscisse et l'animation de la voiture, publie
//! [`StuntOrders`] pour le réalisateur et suspend les événements du trafic le temps de la cascade.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, AudioFeatures, CorePlugin, CullingPlugin};
use cf_world_neon::pilot::PilotCar;
use cf_world_neon::stunts::{
    StuntAlign, StuntAppExt, StuntCtx, StuntEnv, StuntKind, StuntOrders, StuntPose, StuntRegistry, StuntRun,
    StuntScript, StuntTier, StuntWorld, StuntsPlugin,
};
use cf_world_neon::traffic::TrafficPlugin;
use cf_world_neon::{CarAnimation, PilotPlugin, PilotSettings};

const FRAME: Duration = Duration::from_micros(16_667);

/// Un saut de 6 m de 5 s, au-dessus de la conduite normale.
struct Hop;

struct HopRun;

impl StuntRun for HopRun {
    fn duration(&self) -> f64 {
        5.0
    }
    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let u = (t / 5.0).clamp(0.0, 1.0);
        out.x = ctx.env.car_x + 2.0 * (u * std::f64::consts::PI).sin();
        out.altitude = 6.0 * (u * std::f64::consts::PI).sin();
        out.pitch = std::f64::consts::TAU * u;
        out.hover = true;
        t < 5.0
    }
    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}

impl StuntScript for Hop {
    fn kind(&self) -> StuntKind {
        StuntKind::Slalom
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Common
    }
    fn weight(&self) -> f64 {
        1.0
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, _env: &StuntEnv<'_>, _world: &dyn StuntWorld) -> bool {
        true
    }
    fn begin(&self, _ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        Box::new(HopRun)
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        CullingPlugin::default(),
        PilotPlugin {
            settings: PilotSettings::default(),
        },
        TrafficPlugin {
            assets_dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            ..TrafficPlugin::default()
        },
        StuntsPlugin::default(),
    ))
    .init_asset::<Mesh>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures {
        live: true,
        loud: 0.6,
        bass: 0.5,
        intensity: 0.8,
        bpm: Some(128.0),
        kick: false,
        ..cf_analysis::AudioFeatures::NEUTRAL
    }))
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
    // `StuntsPlugin` inscrit les douze vrais scripts : le test ne doit pas dépendre du tirage dans ce répertoire, il
    // joue seulement le faux script (Slalom, remplacé).
    .insert_resource(StuntRegistry::default())
    .register_stunt(Hop);
    app
}

#[test]
fn une_cascade_fond_sa_pose_sur_la_voiture_et_publie_ses_ordres() {
    let mut app = app();
    let mut frames_active = 0;
    let mut max_alt = 0.0f32;
    let mut max_weight = 0.0f32;
    for i in 0..(60 * 240) {
        // Un kick tous les 28 images, comme un morceau à 128 bpm : les cascades partent sur une mesure.
        app.world_mut().resource_mut::<AudioFeatures>().0.kick = i % 28 == 0;
        app.world_mut().resource_mut::<AudioFeatures>().0.count = i / 28;
        app.update();
        let orders = *app.world().resource::<StuntOrders>();
        let animation = *app.world().resource::<CarAnimation>();
        if orders.active() {
            frames_active += 1;
            let view = orders.view.expect("vue");
            assert_eq!(view.kind, StuntKind::Slalom);
            assert!((0.0..=1.0).contains(&view.progress));
        }
        max_alt = max_alt.max(animation.altitude);
        max_weight = max_weight.max(orders.weight);
        assert!(animation.altitude.is_finite() && animation.altitude >= 0.0);
        // La position de la voiture porte l'altitude de l'animation fondue : la voiture, la traînée et la caméra la suivent.
        let world = app.world_mut();
        let mut positions = world.query_filtered::<&AbsolutePosition, With<PilotCar>>();
        let y = positions.single(world).expect("la voiture").0.y;
        assert!(
            (y - f64::from(animation.altitude)).abs() < 1e-3,
            "{y} {}",
            animation.altitude
        );
    }
    assert!(
        frames_active > 60,
        "au moins une cascade a eu lieu : {frames_active} images"
    );
    assert!(max_alt > 2.0, "la pose a soulevé la voiture : {max_alt}");
    assert!(max_weight > 0.9, "la pose a pesé son plein poids : {max_weight}");
}

#[test]
fn sans_signal_audio_aucune_cascade() {
    let mut app = app();
    app.world_mut().resource_mut::<AudioFeatures>().0.live = false;
    for i in 0..(60 * 120) {
        app.world_mut().resource_mut::<AudioFeatures>().0.kick = i % 28 == 0;
        app.update();
        assert!(!app.world().resource::<StuntOrders>().active());
    }
}
