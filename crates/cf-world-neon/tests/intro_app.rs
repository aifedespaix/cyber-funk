//! La pré-intro dans une vraie `App` Bevy, sans rendu : au niveau 0 le monde se construit (route, immeubles, ciel,
//! phares, enseigne, plafond de vitesse) ; quand le spectacle monte, tout s'achève puis revient à 1.

#![allow(clippy::expect_used)]

use std::path::Path;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{CorePlugin, CurrentShow};
use cf_show::director::ShowDirector;
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_world_neon::djsign::DjSignIntro;
use cf_world_neon::intro::{IntroPlugin, IntroStatus};
use cf_world_neon::{CarControls, FacadeLook, PilotSettings, RoadLook, SkyLook};
use rand_pcg::Pcg32;

const FRAME: Duration = Duration::from_micros(16_667);

fn demo_show() -> CurrentShow {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = LookLibrary::load_dir(&assets.join("looks")).expect("looks");
    let show = LoadedShow::load(&assets.join("shows/demo.ron"), &looks).expect("show");
    let director = ShowDirector::<Pcg32>::new(&show, DropTitles::empty(), 1).expect("chef");
    CurrentShow(director.state().clone())
}

fn app(show: Option<CurrentShow>) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin, IntroPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .init_resource::<RoadLook>()
        .init_resource::<FacadeLook>()
        .init_resource::<SkyLook>()
        .init_resource::<DjSignIntro>()
        .init_resource::<CarControls>()
        .init_resource::<PilotSettings>();
    if let Some(show) = show {
        app.insert_resource(show);
    }
    app
}

fn drive(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

fn set_level(app: &mut App, level: u8) {
    app.world_mut().resource_mut::<CurrentShow>().0.level = level;
}

#[test]
fn sans_spectacle_le_monde_reste_entier() {
    let mut app = app(None);
    drive(&mut app, 120);
    assert!((app.world().resource::<RoadLook>().draw - 1.0).abs() < f32::EPSILON);
    assert!((app.world().resource::<FacadeLook>().build - 1.0).abs() < f32::EPSILON);
    assert!((app.world().resource::<SkyLook>().reveal - 1.0).abs() < f32::EPSILON);
    assert!(app.world().resource::<DjSignIntro>().0.is_none());
    assert!(app.world().resource::<PilotSettings>().speed_cap_mps.is_none());
}

#[test]
fn au_niveau_zero_le_monde_se_construit_puis_s_acheve() {
    let mut show = demo_show();
    show.0.level = 0;
    let mut app = app(Some(show));
    drive(&mut app, 5);
    // Tout vient d'être éteint : ciel noir, immeubles à terre, phares éteints, enseigne réclamée, filet de vitesse.
    assert!(app.world().resource::<SkyLook>().reveal < 0.01);
    assert!(app.world().resource::<FacadeLook>().build < 0.01);
    assert!(app.world().resource::<CarControls>().light_scale < 0.01);
    assert!(app.world().resource::<DjSignIntro>().0.is_some());
    let cap = app.world().resource::<PilotSettings>().speed_cap_mps.expect("plafond");
    assert!(cap < 4.0, "{cap}");

    // 25 s plus tard la route est tracée et les immeubles ont monté.
    drive(&mut app, 25 * 60);
    assert!((app.world().resource::<RoadLook>().draw - 1.0).abs() < f32::EPSILON);
    assert!(app.world().resource::<FacadeLook>().build > 0.5);
    assert!(app.world().resource::<IntroStatus>().0.running);

    // Le spectacle monte : quelques secondes, puis plus aucun effet.
    set_level(&mut app, 1);
    drive(&mut app, 4 * 60);
    assert!(!app.world().resource::<IntroStatus>().0.running);
    assert!((app.world().resource::<SkyLook>().reveal - 1.0).abs() < f32::EPSILON);
    assert!((app.world().resource::<FacadeLook>().build - 1.0).abs() < f32::EPSILON);
    assert!((app.world().resource::<CarControls>().light_scale - 1.0).abs() < f32::EPSILON);
    assert!(app.world().resource::<DjSignIntro>().0.is_none());
    assert!(app.world().resource::<PilotSettings>().speed_cap_mps.is_none());
}
