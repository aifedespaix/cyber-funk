//! Le ciel de Neon City dans une vraie `App` Bevy, sans rendu : couche arrière-plan, ciel qui ne bouge pas quand la caméra
//! voyage, `Aabb` justes sans `NoFrustumCulling`, astres qui tournent, étoiles filantes, matériaux nourris par la musique
//! et la palette, géante à anneau exposée.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, CorePlugin, CurrentShowFx, Layer, SceneColor, ScenePalette,
    ShowFxInput, aabb_encloses_mesh, mesh_aabb,
};
use cf_world_neon::sky::{
    DomeMaterial, PLANETS, PlanetMaterial, PlanetTextures, POOL, RADIUS_M, RING_PLANET_TEXTURE, RingPlanet,
    SkyPart, SkyPlanet, SkyState, SkyStreak, StarsMaterial, StreakMaterial,
};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig, ShootingStarRequest, SkyLook};

const FRAME: Duration = Duration::from_micros(16_667);

fn app(start_z_m: f64, speed_mps: f64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig { start_z_m, speed_mps },
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

fn with_audio(app: &mut App) {
    app.insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL));
}

fn streaks_visible(app: &mut App) -> usize {
    let mut query = app.world_mut().query::<(&SkyStreak, &Visibility)>();
    query
        .iter(app.world())
        .filter(|(_, visibility)| **visibility == Visibility::Visible)
        .count()
}

#[test]
fn tout_le_ciel_est_dans_la_couche_arriere_plan() {
    let mut app = app(0.0, 0.0);
    app.update();
    let mut query = app.world_mut().query::<(&SkyPart, &RenderLayers, Option<&Mesh3d>)>();
    let parts: Vec<_> = query.iter(app.world()).map(|(_, l, m)| (l.clone(), m.is_some())).collect();
    // Dôme, étoiles, deux aurores, sept astres, deux anneaux, cinq traits.
    assert_eq!(parts.len(), 1 + 1 + 2 + 7 + 2 + POOL);
    for (layers, has_mesh) in parts {
        assert_eq!(layers, Layer::Background.render_layers());
        assert!(has_mesh);
    }
}

#[test]
fn le_ciel_reste_a_l_infini_quand_la_camera_voyage_a_400_km() {
    let mut app = app(-400_000.0, 40.0);
    app.update();
    let snapshot = |app: &mut App| -> Vec<(Entity, Vec3)> {
        let mut query = app.world_mut().query_filtered::<(Entity, &Transform), (With<SkyPart>, Without<SkyStreak>)>();
        let mut out: Vec<_> = query.iter(app.world()).map(|(e, t)| (e, t.translation)).collect();
        out.sort_by_key(|(e, _)| *e);
        out
    };
    let before = snapshot(&mut app);
    for _ in 0..240 {
        app.update();
    }
    // La caméra (et le point suivi) ont avancé de ~160 m, l'origine flottante a peut-être sauté : le ciel n'a pas bougé d'un
    // millimètre, aucune de ses entités ne porte de position absolue qui l'y entraînerait.
    assert_eq!(snapshot(&mut app), before);
    let mut absolute = app.world_mut().query_filtered::<&AbsolutePosition, With<SkyPart>>();
    assert_eq!(absolute.iter(app.world()).count(), 0);
}

#[test]
fn la_camera_de_fond_se_place_avec_sky_drift_et_jamais_avec_la_position() {
    let drift = Vec3::new(-261.4, 200.7, -452.8);
    for z in [0.0, -12_345.0, -400_000.0] {
        let pose = CameraPose {
            position: DVec3::new(3.0, 2.0, z),
            sky_drift: drift,
            ..CameraPose::default()
        };
        let view = pose.view_for(Layer::Background, DVec3::new(0.0, 0.0, z)).expect("fond");
        // Dans le repère du ciel, la caméra est à `sky_drift` : à zéro elle est au centre (le ciel est à l'infini), sinon elle
        // « voyage » vers les astres.
        assert_eq!(view.translation, drift, "z = {z}");
    }
    let at_rest = CameraPose::default().view_for(Layer::Background, DVec3::ZERO).expect("fond");
    assert_eq!(at_rest.translation, Vec3::ZERO);
}

#[test]
fn les_aabb_sont_justes_et_aucun_mesh_du_ciel_n_evite_le_culling() {
    let mut app = app(0.0, 0.0);
    app.update();
    let mut forced = app.world_mut().query_filtered::<Entity, (With<SkyPart>, With<NoFrustumCulling>)>();
    assert_eq!(forced.iter(app.world()).count(), 0, "pas de NoFrustumCulling dans le ciel");

    let mut query = app.world_mut().query_filtered::<(&Name, &Mesh3d, &Transform), With<SkyPart>>();
    let rows: Vec<_> = query
        .iter(app.world())
        .map(|(n, m, t)| (n.to_string(), m.0.clone(), *t))
        .collect();
    let meshes = app.world().resource::<Assets<Mesh>>();
    for (name, handle, transform) in rows {
        let mesh = meshes.get(&handle).expect("mesh chargé");
        let aabb = mesh_aabb(mesh).expect("positions");
        assert!(aabb_encloses_mesh(&aabb, mesh, 1e-3), "{name}");
        // Volume dans le monde : rien du ciel ne dépasse la voûte, que la caméra de fond (plan lointain à 20 000 m) voit
        // entière. Le trait n'est pas posé tant qu'il ne vole pas : on ne regarde que ce qui est en place.
        if name.contains("filante") {
            continue;
        }
        let reach = Vec3::from(aabb.center).abs() * transform.scale + Vec3::from(aabb.half_extents) * transform.scale;
        let far = reach + transform.translation.abs();
        assert!(far.max_element() <= RADIUS_M * 2.0, "{name} : {far}");
    }
    // Le dôme et les étoiles sont centrés sur l'origine du ciel et remplissent la voûte.
    let dome = {
        let mut q = app.world_mut().query::<(&Name, &Mesh3d)>();
        q.iter(app.world())
            .find(|(n, _)| n.as_str() == "ciel · dôme")
            .map(|(_, m)| m.0.clone())
            .expect("dôme")
    };
    let aabb = mesh_aabb(app.world().resource::<Assets<Mesh>>().get(&dome).expect("mesh")).expect("positions");
    assert!(Vec3::from(aabb.center).length() < 1e-3);
    assert!((aabb.half_extents.x - RADIUS_M).abs() < 1.0 && (aabb.half_extents.y - RADIUS_M).abs() < 1.0);
}

#[test]
fn la_geante_a_anneau_est_exposee_au_realisateur() {
    let mut app = app(0.0, 0.0);
    app.update();
    let ring = *app.world().resource::<RingPlanet>();
    let spec = PLANETS
        .iter()
        .find(|s| s.texture == RING_PLANET_TEXTURE)
        .expect("géante");
    assert_eq!(ring.center, spec.center());
    assert!((ring.radius - spec.radius_m).abs() < f32::EPSILON);
    assert!(ring.ring_inner > ring.radius && ring.ring_outer > ring.ring_inner);
    assert!((ring.ring_normal.length() - 1.0).abs() < 1e-5);
    // Le ciel est dans la voûte : la géante aussi, anneau compris.
    assert!(ring.center.length() + ring.ring_outer < RADIUS_M * 1.5);
    assert_eq!(app.world().resource::<PlanetTextures>().0.len(), PLANETS.len());
}

#[test]
fn les_astres_tournent_sur_leur_axe() {
    let mut app = app(0.0, 0.0);
    app.update();
    for _ in 0..60 {
        app.update();
    }
    let seconds = 60.0 * FRAME.as_secs_f32();
    let mut planets = app.world_mut().query::<(&SkyPlanet, &Transform)>();
    let rows: Vec<_> = planets.iter(app.world()).map(|(p, t)| (p.0, *t)).collect();
    let state = app.world().resource::<SkyState>();
    assert_eq!(rows.len(), PLANETS.len());
    for (index, transform) in rows {
        let spec = &PLANETS[index];
        let expected = spec.spin_step_rad(seconds);
        let spin = state.spin_rad(index);
        // Une seconde environ de rotation (la première image n'a pas de delta).
        assert!((spin - expected).abs() < expected * 0.05, "{} : {spin} contre {expected}", spec.texture);
        assert!(transform.rotation.angle_between(spec.rotation(spin)) < 1e-3);
        assert!((transform.scale.x - spec.radius_m).abs() < f32::EPSILON);
        assert!(transform.translation.distance(spec.center()) < 1e-3, "l'astre ne se déplace pas");
    }
}

#[test]
fn les_etoiles_filantes_partent_vite_se_cachent_et_respectent_la_cadence() {
    let mut app = app(0.0, 0.0);
    app.update();
    assert_eq!(streaks_visible(&mut app), 0, "aucun trait au départ");
    let mut seen = 0;
    let mut starts = Vec::new();
    let mut was_visible = 0;
    for frame in 0..(30.0 / FRAME.as_secs_f32()) as u32 {
        app.update();
        let visible = streaks_visible(&mut app);
        assert!(visible <= POOL);
        seen = seen.max(visible);
        if visible > was_visible {
            starts.push(f64::from(frame) * FRAME.as_secs_f64());
        }
        was_visible = visible;
    }
    assert!(seen >= 1, "au moins une étoile en 30 s");
    assert!(starts.len() >= 3, "{starts:?}");
    for window in starts.windows(4) {
        assert!(window[3] - window[0] >= 1.0, "plus de 3 départs en une seconde : {window:?}");
    }
    for pair in starts.windows(2) {
        assert!(pair[1] - pair[0] >= 0.25, "départs trop rapprochés : {pair:?}");
    }
}

#[test]
fn un_ordre_lance_une_etoile_tout_de_suite_mais_la_cadence_le_borne() {
    let mut app = app(0.0, 0.0);
    app.update();
    // 20 ordres d'un coup : au plus 3 partent (cadence), au plus 1 pour les 0,25 s d'écart.
    for _ in 0..20 {
        app.world_mut().write_message(ShootingStarRequest);
    }
    app.update();
    assert_eq!(streaks_visible(&mut app), 1);
}

#[test]
fn un_kick_sur_vingt_quatre_lance_une_etoile_sauf_flashs_coupes() {
    let kick = |app: &mut App, count: u32| {
        app.world_mut().resource_mut::<AudioFeatures>().0.count = count;
        app.update();
    };
    // Sans spectacle : le 24e kick lance une étoile.
    let mut app_free = app(0.0, 0.0);
    with_audio(&mut app_free);
    app_free.update();
    kick(&mut app_free, 23);
    assert_eq!(streaks_visible(&mut app_free), 0);
    kick(&mut app_free, 24);
    assert_eq!(streaks_visible(&mut app_free), 1);

    // Flashs coupés par le show (`FlashKill`) : aucun départ sur le kick.
    let mut app_safe = app(0.0, 0.0);
    with_audio(&mut app_safe);
    app_safe.insert_resource(CurrentShowFx(ShowFxInput {
        flashes: false,
        breathing: false,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    }));
    app_safe.update();
    kick(&mut app_safe, 23);
    kick(&mut app_safe, 24);
    assert_eq!(streaks_visible(&mut app_safe), 0);
}

#[test]
fn les_materiaux_suivent_le_pouls_la_palette_et_le_fondu() {
    let mut app = app(0.0, 0.0);
    with_audio(&mut app);
    app.world_mut().resource_mut::<SkyLook>().reveal = 0.25;
    app.world_mut().resource_mut::<AudioFeatures>().0.pulse = 0.8;
    app.update();
    app.update();
    let palette = *app.world().resource::<ScenePalette>();
    let ink = palette.get(SceneColor::Ink);

    let dome = app.world().resource::<Assets<DomeMaterial>>();
    let (_, dome) = dome.iter().next().expect("un dôme");
    assert!((dome.params.frame.y - 0.8).abs() < 1e-6, "pouls");
    assert!((dome.params.frame.z - 0.25).abs() < 1e-6, "fondu");
    assert!((dome.params.ink.x - ink.r).abs() < 1e-6 && (dome.params.ink.z - ink.b).abs() < 1e-6);
    assert!(dome.params.frame.x > 0.0, "le temps avance");

    let stars = app.world().resource::<Assets<StarsMaterial>>();
    let (_, stars) = stars.iter().next().expect("des étoiles");
    let ice = palette.get(SceneColor::Ice);
    assert!((stars.params.tints[0].x - ice.r).abs() < 1e-6 && (stars.params.tints[1].x - ice.r).abs() < 1e-6);
    assert!((stars.params.frame.z - 0.25).abs() < 1e-6);

    let planets = app.world().resource::<Assets<PlanetMaterial>>();
    assert_eq!(planets.iter().count(), PLANETS.len());
    for (_, planet) in planets.iter() {
        assert!((planet.params.frame.x - 0.8).abs() < 1e-6 && (planet.params.frame.y - 0.25).abs() < 1e-6);
        assert!(planet.map.is_some(), "texture posée");
        assert!((planet.params.light.truncate().length() - 1.0).abs() < 1e-5);
    }

    // Un trait libre est invisible : opacité nulle, mais le fondu lui arrive aussi.
    let streaks = app.world().resource::<Assets<StreakMaterial>>();
    assert_eq!(streaks.iter().count(), POOL);
    assert!(streaks.iter().all(|(_, s)| s.params.frame.x == 0.0 && (s.params.frame.y - 0.25).abs() < 1e-6));
}

#[test]
fn le_mode_calme_ralentit_les_aurores() {
    let mut app = app(0.0, 0.0);
    app.update();
    let motion = |app: &App| -> Vec<f32> {
        let aurora = app.world().resource::<Assets<cf_world_neon::sky::AuroraMaterial>>();
        aurora.iter().map(|(_, a)| a.params.frame.w).collect()
    };
    app.update();
    assert!(motion(&app).iter().all(|m| (m - 1.0).abs() < f32::EPSILON));
    app.world_mut().resource_mut::<SkyLook>().calm = true;
    app.update();
    assert!(motion(&app).iter().all(|m| (m - 0.3).abs() < 1e-6));
}
