//! Le repli de qualité basse du moteur de quartiers, dans une vraie `App` Bevy sans rendu : l'étalonnage glisse vers le
//! neutre en 1,5 s (rampe linéaire, jamais d'à-coup) et en revient de même ; la ville, elle, continue de changer de
//! quartier (la géométrie ne coûte pas plus cher). Temps manuel à 10 i/s.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{
    AbsolutePosition, AudioFeatures, CorePlugin, DistrictGradeState, DistrictOrder, DistrictState, PalettePlugin,
    QualityLevers, StreamFocus, levers_for,
};
use cf_show::district::{DistrictId, DistrictKind, TransitionKind};
use cf_world_neon::facade::LOW_QUALITY_FADE_S;
use cf_world_neon::{DistrictsPlugin, FacadeLook, NeonWorld};

const FRAME_S: f64 = 0.1;
/// Vitesse de la voiture, m/s.
const SPEED: f64 = 60.0;
/// Pas maximal du repli par image (rampe linéaire de 1,5 s).
const STEP: f32 = FRAME_S as f32 / LOW_QUALITY_FADE_S;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, CorePlugin))
        .init_asset::<Mesh>()
        .init_resource::<AudioFeatures>()
        .init_resource::<FacadeLook>()
        .insert_resource(NeonWorld::default())
        .insert_resource(levers_for(0))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(FRAME_S)))
        .add_plugins((PalettePlugin, DistrictsPlugin));
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    app.update();
    app
}

/// Une image : la voiture avance, un kick toutes les 5 images.
fn frame(app: &mut App, i: usize) {
    {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut position in cars.iter_mut(world) {
            position.0.z -= SPEED * FRAME_S;
        }
    }
    {
        let mut audio = app.world_mut().resource_mut::<AudioFeatures>();
        audio.0.live = true;
        audio.0.time_s = i as f64 * FRAME_S;
        audio.0.kick = i.is_multiple_of(5);
        audio.0.count = (i / 5) as u32;
        audio.0.bpm = Some(120.0);
    }
    app.update();
}

fn grade(app: &App) -> &DistrictGradeState {
    app.world().resource::<DistrictGradeState>()
}

fn state(app: &App) -> DistrictState {
    *app.world().resource::<DistrictState>()
}

/// Force `to` et joue jusqu'à la fin de la transition (ville et étalonnage). Rend l'image suivante.
fn change_to(app: &mut App, from: usize, to: DistrictId, mut each: impl FnMut(&App)) -> usize {
    app.world_mut().write_message(DistrictOrder::Force {
        to,
        transition: TransitionKind::Gradient,
    });
    for i in from..from + 900 {
        frame(app, i);
        each(app);
        let g = grade(app);
        if state(app).target.is_none() && !g.is_pending() && !g.is_fading() {
            assert_eq!(state(app).current, to);
            return i + 1;
        }
    }
    panic!("la transition vers {to:?} ne finit pas");
}

/// Joue la rampe du repli et vérifie qu'elle est linéaire, sans à-coup, et qu'elle dure 1,5 s. Rend l'image suivante.
fn ramp(app: &mut App, from: usize, low: bool) -> usize {
    app.insert_resource(levers_for(if low { 2 } else { 0 }));
    let target = if low { 1.0 } else { 0.0 };
    let mut last = grade(app).low_quality_mix();
    let mut last_hue = grade(app).displayed().hue_deg;
    let full_hue = grade(app).settled().hue_deg.abs();
    for i in from..from + 40 {
        frame(app, i);
        let g = grade(app);
        let mix = g.low_quality_mix();
        assert!((mix - last).abs() <= STEP + 1e-5, "à-coup du repli : {last} -> {mix}");
        assert!(if low { mix >= last } else { mix <= last }, "le repli recule");
        let hue = g.displayed().hue_deg;
        assert!(
            (hue - last_hue).abs() <= full_hue * STEP + 1e-4,
            "à-coup de teinte : {last_hue} -> {hue}"
        );
        last = mix;
        last_hue = hue;
        if (mix - target).abs() < f32::EPSILON {
            let frames = i + 1 - from;
            let duration = frames as f64 * FRAME_S;
            assert!(
                (duration - f64::from(LOW_QUALITY_FADE_S)).abs() <= FRAME_S + 1e-6,
                "rampe de {duration} s"
            );
            return i + 1;
        }
    }
    panic!("le repli n'atteint jamais {target}");
}

#[test]
fn qualite_basse_coupe_l_etalonnage_progressivement() {
    let mut app = app();
    let suburb = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 3,
    };
    let mut i = change_to(&mut app, 1, suburb, |_| {});
    let graded = grade(&app).displayed();
    assert!(!graded.is_neutral(), "la banlieue a un étalonnage");
    assert!(grade(&app).low_quality_mix().abs() < f32::EPSILON);

    // Qualité basse : l'étalonnage glisse vers le neutre en 1,5 s.
    i = ramp(&mut app, i, true);
    assert!(
        grade(&app).displayed().is_neutral(),
        "neutre à l'octet près en repli complet"
    );

    // Les quartiers continuent de changer : la ville passe au résidentiel, l'étalonnage montré reste neutre, celui de
    // dessous suit le quartier.
    let residential = DistrictId {
        kind: DistrictKind::Residential,
        variant: 1,
    };
    i = change_to(&mut app, i, residential, |app| {
        assert!(
            grade(app).displayed().is_neutral(),
            "l'étalonnage revient en qualité basse"
        );
    });
    let below = grade(&app).settled();
    assert_eq!(below, grade(&app).library().grade(residential));
    {
        let world = app.world().resource::<NeonWorld>();
        let plan = world.scenery().districts();
        assert_eq!(plan.district_at(world.scenery().last_planned_index()), residential);
    }

    // La qualité revient : l'étalonnage du quartier courant remonte en 1,5 s, sans à-coup.
    ramp(&mut app, i, false);
    assert_eq!(grade(&app).displayed(), below);
    assert!(app.world().resource::<QualityLevers>().level < 2);
}
