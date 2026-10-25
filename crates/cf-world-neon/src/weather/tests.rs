//! La météo dans une application Bevy sans rendu : le câblage (route, objectif), la qualité adaptative et la brume.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::postfx::LensWeather;
use cf_render::quality::levers_for;
use cf_render::{FloatingOrigin, MAX_LEVEL, QualityLevers, ScenePalette};

use super::mist::{MistMaterial, MistRig, spawn_mist};
use super::rain::{Rain, RainMaterial, RainRig, spawn_rain};
use super::systems::{WeatherState, step_weather, update_mist, update_rain};
use super::{Thunder, WeatherSettings};
use crate::RoadLook;

const DT_S: f32 = 1.0 / 30.0;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT_S)))
        .init_asset::<Mesh>()
        .init_asset::<MistMaterial>()
        .init_asset::<RainMaterial>()
        .init_resource::<WeatherSettings>()
        .init_resource::<RoadLook>()
        .init_resource::<LensWeather>()
        .init_resource::<ScenePalette>()
        .init_resource::<FloatingOrigin>()
        .insert_resource(WeatherState::default())
        .add_message::<Thunder>()
        .add_systems(Startup, (spawn_mist, spawn_rain))
        .add_systems(Update, (step_weather, (update_mist, update_rain).chain()).chain());
    app.update();
    app
}

fn rain_density_in_material(app: &App) -> f32 {
    let rig = app.world().resource::<RainRig>();
    let materials = app.world().resource::<Assets<RainMaterial>>();
    materials.get(&rig.material).map_or(-1.0, |m| m.params.car.w)
}

fn rain_visibility(app: &mut App) -> Visibility {
    let mut query = app.world_mut().query_filtered::<&Visibility, With<Rain>>();
    *query.single(app.world()).expect("l'entité de la pluie existe")
}

#[test]
fn la_pluie_et_la_brume_sont_creees_une_fois_et_cachees_au_repos() {
    let mut app = app();
    assert_eq!(rain_visibility(&mut app), Visibility::Hidden, "pas d'averse : cachée");
    assert!(app.world().get_resource::<MistRig>().is_some());
    let entities = app.world().entities().len();
    for _ in 0..600 {
        app.update();
    }
    assert_eq!(
        app.world().entities().len(),
        entities,
        "aucune entité de plus en régime établi"
    );
}

#[test]
fn l_averse_arrive_sur_la_route_et_l_objectif_puis_le_ciel_se_vide() {
    let mut app = app();
    let (mut max_road, mut max_lens, mut shown) = (0.0_f32, 0.0_f32, false);
    for _ in 0..(120.0 / DT_S) as usize {
        app.update();
        max_road = max_road.max(app.world().resource::<RoadLook>().rain);
        max_lens = max_lens.max(app.world().resource::<LensWeather>().rain);
    }
    // Une averse libre a eu lieu : la route est mouillée, l'objectif l'a vue.
    assert!(
        max_road > 0.9 && max_lens > 0.9,
        "route {max_road}, objectif {max_lens}"
    );
    app.world_mut().resource_mut::<WeatherSettings>().enabled = false;
    for _ in 0..(40.0 / DT_S) as usize {
        app.update();
    }
    assert!(app.world().resource::<RoadLook>().rain < 0.01);
    shown |= rain_visibility(&mut app) == Visibility::Inherited;
    assert!(!shown, "plus d'averse : la pluie est cachée");
}

#[test]
fn sans_spectacle_le_tonnerre_n_allume_aucun_eclair() {
    let mut app = app();
    let mut thunders = 0;
    for _ in 0..(900.0 / DT_S) as usize {
        app.update();
        thunders += app.world_mut().resource_mut::<Messages<Thunder>>().drain().count();
        let level = app.world().resource::<WeatherState>().lightning.level();
        assert!(level == 0.0, "règle 7 : sans spectacle, aucun éclair ({level})");
    }
    assert!(thunders > 0, "le tonnerre gronde quand même");
}

#[test]
fn la_qualite_adaptative_allege_la_pluie_et_la_brume() {
    let mut app = app();
    app.world_mut().resource_mut::<WeatherState>().weather.rain = 1.0;
    // Une pluie constante : on fige le cycle en gardant l'averse chargée.
    let mut density = Vec::new();
    for level in 0..=MAX_LEVEL {
        app.insert_resource(levers_for(level));
        // Plusieurs images : la météo décroît d'une image à l'autre, on mesure le rapport à la pluie du moment.
        app.world_mut().resource_mut::<WeatherState>().weather.rain = 1.0;
        app.update();
        let rain = app.world().resource::<WeatherState>().weather.rain;
        density.push(rain_density_in_material(&app) / rain.max(1e-3));
    }
    assert!(density[0] > 0.95, "plein régime : toutes les gouttes ({})", density[0]);
    assert!(
        density[MAX_LEVEL] < 0.55 * density[0],
        "dernier cran : moitié des gouttes ({density:?})"
    );
    assert!(
        density.windows(2).all(|w| w[1] <= w[0] + 1e-3),
        "monotone : {density:?}"
    );

    // La brume : bruit allégé dès que la qualité retire des particules.
    let mist = |app: &App| {
        let rig = app.world().resource::<MistRig>();
        app.world()
            .resource::<Assets<MistMaterial>>()
            .get(&rig.material)
            .map(|m| m.params.frame.w)
    };
    app.insert_resource(levers_for(0));
    app.update();
    assert_eq!(mist(&app), Some(0.0));
    app.insert_resource(levers_for(MAX_LEVEL));
    app.update();
    assert_eq!(mist(&app), Some(1.0));
    assert_eq!(app.world().resource::<QualityLevers>().level, MAX_LEVEL);
}
