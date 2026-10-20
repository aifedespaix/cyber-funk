//! Les systèmes du culling dans une vraie `App` Bevy, sans rendu : l'occlusion sur la caméra du monde et son réglage,
//! la cadence de simulation selon visibilité et distance, les plages de visibilité sur la ville de boîtes.
//!
//! Ce que fait le GPU de l'occlusion (objets réellement non dessinés) ne se vérifie pas ici : c'est le travail de
//! `examples/culling_city.rs`. Ces tests tiennent le câblage : qui reçoit quoi, quand, et rien en régime établi.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::visibility::{ViewVisibility, VisibilityRange};
use bevy::camera::{Camera, Projection};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::render::occlusion_culling::OcclusionCulling;
use bevy::time::TimeUpdateStrategy;
use cf_render::streaming::StreamedPiece;
use cf_render::{
    AbsolutePosition, BoxWorld, Cadence, CorePlugin, CullingPlugin, CullingSettings, CullingView, FOG_FULL_M,
    FloatingOrigin, Layer, LayerCamera, LayersPlugin, SimulationLod, StreamFocus, WorldPlugin,
};

const FRAME: Duration = Duration::from_micros(16_667);

fn base_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), CorePlugin))
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<SkinnedMeshInverseBindposes>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

fn rig_app(settings: CullingSettings) -> App {
    let mut app = base_app();
    app.init_resource::<FloatingOrigin>()
        .add_plugins((LayersPlugin::default(), CullingPlugin { settings }));
    app.update();
    app
}

fn camera_of(app: &mut App, layer: Layer) -> Entity {
    app.world_mut()
        .query::<(Entity, &LayerCamera)>()
        .iter(app.world())
        .find(|(_, camera)| camera.0 == layer)
        .map(|(entity, _)| entity)
        .expect("caméra de couche")
}

fn has_occlusion(app: &App, camera: Entity) -> (bool, bool) {
    let entity = app.world().entity(camera);
    (entity.contains::<OcclusionCulling>(), entity.contains::<DepthPrepass>())
}

#[test]
fn seule_la_camera_du_monde_recoit_l_occlusion_et_son_prepass() {
    let mut app = rig_app(CullingSettings::default());
    let world = camera_of(&mut app, Layer::World);
    assert!(app.world().entity(world).contains::<CullingView>());
    assert_eq!(has_occlusion(&app, world), (true, true));
    for layer in [Layer::Background, Layer::Foreground, Layer::Overlay] {
        let camera = camera_of(&mut app, layer);
        assert!(!app.world().entity(camera).contains::<CullingView>(), "{layer:?}");
        assert_eq!(has_occlusion(&app, camera), (false, false), "{layer:?}");
    }
    // Le compositeur (ses quads plein cadre n'occultent rien) n'en reçoit pas non plus.
    let cameras = app
        .world_mut()
        .query_filtered::<Entity, With<Camera>>()
        .iter(app.world())
        .count();
    let equipped = app
        .world_mut()
        .query_filtered::<Entity, With<OcclusionCulling>>()
        .iter(app.world())
        .count();
    assert_eq!((cameras, equipped), (5, 1));
}

#[test]
fn le_reglage_coupe_et_rallume_l_occlusion() {
    let mut app = rig_app(CullingSettings::default());
    let world = camera_of(&mut app, Layer::World);

    app.world_mut().resource_mut::<CullingSettings>().occlusion = false;
    app.update();
    assert_eq!(has_occlusion(&app, world), (false, false));

    app.world_mut().resource_mut::<CullingSettings>().occlusion = true;
    app.update();
    assert_eq!(has_occlusion(&app, world), (true, true));
}

#[test]
fn l_occlusion_peut_demarrer_coupee() {
    let mut app = rig_app(CullingSettings {
        occlusion: false,
        ..default()
    });
    let world = camera_of(&mut app, Layer::World);
    assert_eq!(has_occlusion(&app, world), (false, false));
}

#[test]
fn un_prepass_pose_par_un_autre_n_est_jamais_retire() {
    let mut app = rig_app(CullingSettings::default());
    // Une caméra de culling dont le prépasse de profondeur a une autre raison d'être (une passe de post-process).
    let camera = app
        .world_mut()
        .spawn((Camera3d::default(), CullingView, DepthPrepass))
        .id();
    app.update();
    assert_eq!(has_occlusion(&app, camera), (true, true));
    app.world_mut().resource_mut::<CullingSettings>().occlusion = false;
    app.update();
    assert_eq!(
        has_occlusion(&app, camera),
        (false, true),
        "le prépasse d'un autre reste"
    );
}

#[test]
fn rien_ne_bouge_en_regime_etabli() {
    let mut app = rig_app(CullingSettings::default());
    let world = camera_of(&mut app, Layer::World);
    // On retire l'occlusion « dans le dos » du système : s'il retravaillait à chaque image, il la remettrait.
    app.world_mut().entity_mut(world).remove::<OcclusionCulling>();
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(
        has_occlusion(&app, world),
        (false, true),
        "le système a travaillé sans changement de réglage"
    );
}

#[test]
fn une_camera_de_culling_ajoutee_plus_tard_est_equipee() {
    let mut app = rig_app(CullingSettings::default());
    let camera = app.world_mut().spawn((Camera3d::default(), CullingView)).id();
    app.update();
    assert_eq!(has_occlusion(&app, camera), (true, true));
}

// --- Cadence de simulation -------------------------------------------------------------------------------------

struct Probe {
    entity: Entity,
    turns: u32,
    integrated_s: f32,
}

fn lod_app(settings: CullingSettings, with_viewer: bool) -> App {
    let mut app = base_app();
    app.add_plugins(CullingPlugin { settings });
    if with_viewer {
        app.world_mut().spawn((CullingView, GlobalTransform::default()));
    }
    app
}

fn spawn_probe(app: &mut App, distance_m: f32, visible: bool) -> Probe {
    let entity = app
        .world_mut()
        .spawn((
            SimulationLod::default(),
            if visible {
                ViewVisibility::VISIBLE
            } else {
                ViewVisibility::HIDDEN
            },
            GlobalTransform::from_xyz(0.0, 0.0, -distance_m),
        ))
        .id();
    Probe {
        entity,
        turns: 0,
        integrated_s: 0.0,
    }
}

fn run(app: &mut App, probes: &mut [Probe], frames: u32) {
    for _ in 0..frames {
        app.update();
        for probe in probes.iter_mut() {
            let lod = app.world().get::<SimulationLod>(probe.entity).expect("SimulationLod");
            if lod.due() {
                probe.turns += 1;
                probe.integrated_s += lod.step_s();
            }
        }
    }
}

#[test]
fn la_cadence_suit_la_visibilite_et_la_distance_dans_l_image() {
    let mut app = lod_app(CullingSettings::default(), true);
    // Bornes par défaut : 60 m (plein), 200 m (demi), au-delà (quart) à l'écran ; hors champ : huitième sous 150 m,
    // gel au-delà.
    let mut probes = [
        spawn_probe(&mut app, 10.0, true),
        spawn_probe(&mut app, 100.0, true),
        spawn_probe(&mut app, 500.0, true),
        spawn_probe(&mut app, 100.0, false),
        spawn_probe(&mut app, 1_000.0, false),
    ];
    run(&mut app, &mut probes, 64);
    let turns: Vec<u32> = probes.iter().map(|p| p.turns).collect();
    assert_eq!(turns, vec![64, 32, 16, 8, 0]);
    let cadences: Vec<Cadence> = probes
        .iter()
        .map(|p| app.world().get::<SimulationLod>(p.entity).expect("lod").cadence())
        .collect();
    assert_eq!(
        cadences,
        vec![
            Cadence::Full,
            Cadence::Half,
            Cadence::Quarter,
            Cadence::Eighth,
            Cadence::Frozen
        ]
    );
}

#[test]
fn le_temps_integre_par_un_objet_ralenti_est_celui_de_la_scene() {
    let mut app = lod_app(CullingSettings::default(), true);
    let mut probes = [spawn_probe(&mut app, 100.0, true), spawn_probe(&mut app, 10.0, true)];
    run(&mut app, &mut probes, 120);
    let (half, full) = (probes[0].integrated_s, probes[1].integrated_s);
    // Cadence ralentie : le temps intégré ne retarde que de l'image en attente.
    assert!((full - half).abs() <= FRAME.as_secs_f32() * 1.5, "{half} / {full}");
    // La première image de `Time` a un delta nul : 119 images de temps dans 120 mises à jour.
    assert!((full - 119.0 * FRAME.as_secs_f32()).abs() < 1e-3, "{full}");
}

#[test]
fn un_objet_qui_entre_dans_le_champ_repart_a_pleine_cadence() {
    let mut app = lod_app(CullingSettings::default(), true);
    let mut probes = [spawn_probe(&mut app, 30.0, false)];
    run(&mut app, &mut probes, 16);
    assert_eq!(probes[0].turns, 2, "hors champ à 30 m : un huitième");
    app.world_mut()
        .entity_mut(probes[0].entity)
        .insert(ViewVisibility::VISIBLE);
    run(&mut app, &mut probes, 16);
    assert_eq!(probes[0].turns, 2 + 16, "dans le champ à 30 m : chaque image");
}

#[test]
fn le_reglage_desactive_la_cadence() {
    let mut app = lod_app(
        CullingSettings {
            simulation_lod: false,
            ..default()
        },
        true,
    );
    let mut probes = [
        spawn_probe(&mut app, 1_000.0, false),
        spawn_probe(&mut app, 500.0, true),
    ];
    run(&mut app, &mut probes, 32);
    assert_eq!((probes[0].turns, probes[1].turns), (32, 32));
}

#[test]
fn sans_camera_de_culling_tout_reste_a_pleine_cadence() {
    let mut app = lod_app(CullingSettings::default(), false);
    let mut probes = [spawn_probe(&mut app, 1_000.0, false)];
    run(&mut app, &mut probes, 32);
    assert_eq!(probes[0].turns, 32);
}

#[test]
fn les_objets_d_une_meme_cadence_ne_tombent_pas_tous_sur_la_meme_image() {
    let mut app = lod_app(CullingSettings::default(), true);
    let mut probes: Vec<Probe> = (0..400).map(|_| spawn_probe(&mut app, 100.0, false)).collect();
    let mut per_frame = Vec::new();
    for _ in 0..16 {
        let before: u32 = probes.iter().map(|p| p.turns).sum();
        run(&mut app, &mut probes, 1);
        per_frame.push(probes.iter().map(|p| p.turns).sum::<u32>() - before);
    }
    // 400 objets à un tour sur huit : 50 par image en moyenne, jamais tous d'un coup.
    assert!(per_frame.iter().all(|&n| n < 200), "{per_frame:?}");
    assert!(per_frame.iter().filter(|&&n| n > 0).count() >= 6, "{per_frame:?}");
}

// --- Plages de visibilité sur la ville -----------------------------------------------------------------------

fn city_app(world: BoxWorld) -> App {
    let mut app = base_app();
    app.add_plugins((TransformPlugin, bevy::camera::CameraPlugin, WorldPlugin::new(world)));
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(bevy::math::DVec3::new(0.0, 1.0, 0.0))));
    app.world_mut().spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 1.6,
            aspect_ratio: 16.0 / 9.0,
            near: 0.1,
            far: 2_600.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 25.0, -50.0),
    ));
    app.update();
    app.update();
    app
}

fn visible_count(app: &mut App) -> usize {
    app.world_mut()
        .query::<(&StreamedPiece, &ViewVisibility)>()
        .iter(app.world())
        .filter(|(_, v)| v.get())
        .count()
}

#[test]
fn la_plage_de_brume_retire_les_immeubles_lointains_et_eux_seuls() {
    let mut with_range = city_app(BoxWorld::new(11).with_rows_per_chunk(13));
    let mut without = city_app(BoxWorld::new(11).with_rows_per_chunk(13).with_fog_cull(None));
    let (a, b) = (visible_count(&mut with_range), visible_count(&mut without));
    assert!(a > 500, "la ville doit être à l'écran : {a}");
    assert!(a < b, "avec plage : {a} entités visibles, sans : {b}");

    // Au-delà de la brume : rien de visible avec la plage ; sans elle, des immeubles y sont dessinés (le témoin).
    let beyond = |app: &mut App| {
        let eye = Vec3::new(0.0, 25.0, -50.0);
        app.world_mut()
            .query::<(&StreamedPiece, &GlobalTransform, &ViewVisibility)>()
            .iter(app.world())
            // Famille 0 : les immeubles (les seuls à plage).
            .filter(|(piece, t, v)| piece.archetype.index() == 0 && v.get() && t.translation().distance(eye) >= FOG_FULL_M)
            .count()
    };
    assert_eq!(beyond(&mut with_range), 0);
    let drawn_beyond_without = beyond(&mut without);
    assert!(
        drawn_beyond_without > 100,
        "témoin : {drawn_beyond_without} immeubles au-delà de la brume sans plage"
    );
    let ranged_without = without
        .world_mut()
        .query::<&VisibilityRange>()
        .iter(without.world())
        .count();
    assert_eq!(ranged_without, 0, "sans coupe de brume, aucune pièce n'a de plage");
}

#[test]
fn la_coupe_de_brume_est_reglable_et_ne_touche_que_les_immeubles() {
    let world = BoxWorld::new(2);
    assert_eq!(world.fog_cull_m(), Some(FOG_FULL_M));
    assert_eq!(world.clone().with_fog_cull(Some(150.0)).fog_cull_m(), Some(150.0));
    let mut app = city_app(world);
    let ranged = app
        .world_mut()
        .query::<(&StreamedPiece, &VisibilityRange)>()
        .iter(app.world())
        .map(|(piece, _)| piece.archetype.index())
        .collect::<std::collections::BTreeSet<_>>();
    // Famille 0 : les immeubles. Ni les tours-repères (1) ni les voitures (2), qui sont des sujets de caméra.
    assert_eq!(ranged.into_iter().collect::<Vec<_>>(), vec![0]);
    let cameras = app.world_mut().query::<&Camera>().iter(app.world()).count();
    assert_eq!(cameras, 1);
}
