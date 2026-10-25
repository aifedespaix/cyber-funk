//! Les immeubles-vitrines dans une vraie `App` Bevy, sans rendu : entités préallouées dans la couche monde, façade tournée
//! vers la route des deux côtés, et l'événement `landmark` du trafic (inscrit, sujet `Landmark` publié, structure à esquiver,
//! rangement) pour chacun des neuf immeubles.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::time::Duration;

use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::world::SubjectKind;
use cf_render::{AbsolutePosition, CorePlugin, Layer, OriginPlugin, StreamFocus};
use cf_world_neon::landmarks::pass::{LandmarkPass, PassStart};
use cf_world_neon::landmarks::render::LandmarkRig;
use cf_world_neon::traffic::{EventName, EventRegistry, TrafficDirector, TrafficPlugin};
use cf_world_neon::{CityField, LandmarkHandle, LandmarkKind, LandmarksPlugin, PilotTraffic};

const FRAME: Duration = Duration::from_micros(16_667);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        OriginPlugin,
        TrafficPlugin {
            assets_dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            ..TrafficPlugin::default()
        },
        LandmarksPlugin::always(),
    ))
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    // La voiture roule à 60 m/s vers −z, à 400 km du départ.
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    // `App::run` appelle `finish` et `cleanup` ; en pilotant les images à la main, c'est à nous (l'événement `landmark` est
    // inscrit dans `LandmarksPlugin::finish`).
    app.finish();
    app.cleanup();
    app
}

fn drive(app: &mut App, frames: usize) {
    for _ in 0..frames {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut position in cars.iter_mut(world) {
            position.0.z -= 60.0 * FRAME.as_secs_f64();
        }
        app.update();
    }
}

fn entity_count(app: &mut App) -> usize {
    app.world_mut().query::<Entity>().iter(app.world()).count()
}

#[test]
fn les_entites_sont_preallouees_dans_la_couche_monde_et_ne_changent_plus() {
    let mut app = app();
    drive(&mut app, 5);
    let rig = app.world().resource::<LandmarkRig>();
    let count = rig.entity_count();
    // 9 racines, 1 corps par immeuble qui en a un (7), et les pièces : 3 + 3 + 3 + 2 + 6 + 2 + 1 + 1 + 84 = 105... voir la PR.
    assert!(count > 100, "{count} entités");
    let world_layer = Layer::World.render_layers();
    let mut all: Vec<Entity> = Vec::new();
    for kind in LandmarkKind::ALL {
        all.push(rig.root(kind));
        all.extend(rig.parts(kind));
        all.extend(rig.body(kind));
    }
    assert_eq!(all.len(), count);
    for entity in all {
        let layers = app.world().get::<RenderLayers>(entity).expect("RenderLayers");
        assert_eq!(layers, &world_layer, "{entity:?} n'est pas dans la couche monde");
    }
    // Au repos, tout est caché.
    for kind in LandmarkKind::ALL {
        let root = app.world().resource::<LandmarkRig>().root(kind);
        assert_eq!(app.world().get::<Visibility>(root), Some(&Visibility::Hidden));
    }
    let before = entity_count(&mut app);
    // Une minute et demie de passages forcés : aucun spawn ni despawn.
    for kind in LandmarkKind::ALL {
        app.world().resource::<LandmarkHandle>().force(kind, Some((60.0, 1)));
        app.world_mut()
            .resource_mut::<TrafficDirector>()
            .force(EventName::Landmark);
        drive(&mut app, 60 * 5);
    }
    assert_eq!(entity_count(&mut app), before, "aucun spawn en régime établi");
}

#[test]
fn la_facade_regarde_la_route_et_le_corps_s_etend_vers_l_exterieur() {
    for side in [-1_i8, 1] {
        let mut app = app();
        drive(&mut app, 3);
        let car = app
            .world_mut()
            .query_filtered::<&AbsolutePosition, With<StreamFocus>>()
            .single(app.world())
            .expect("voiture")
            .0;
        {
            let handle = app.world().resource::<LandmarkHandle>().clone();
            let mut stage = handle.lock();
            LandmarkPass::begin(
                &mut stage,
                LandmarkKind::Lava,
                car.z,
                PassStart {
                    side,
                    tint: 0,
                    scale: 1.0,
                    debug_center: Some(100.0),
                },
            );
        }
        // Un pas pour poser la racine, un autre pour propager les transforms.
        drive(&mut app, 2);
        let rig = app.world().resource::<LandmarkRig>();
        let root = rig.root(LandmarkKind::Lava);
        assert_eq!(app.world().get::<Visibility>(root), Some(&Visibility::Inherited));
        let root_transform = app.world().get::<GlobalTransform>(root).expect("racine");
        let facade_x = root_transform.translation().x;
        // Le corps s'étend vers −Z local (le sens de sa profondeur) : il doit s'éloigner de l'axe de la route.
        let inside = root_transform.rotation() * Vec3::NEG_Z;
        let s = f32::from(side);
        assert!((facade_x * s - 17.0).abs() < 1e-3, "façade à {facade_x}");
        assert!(inside.x * s > 0.99, "le corps s'éloigne de la route : {inside:?}");
        // Les fenêtres de la façade regardent la route : le plan de la lave est tourné vers l'axe.
        let facade_part = app
            .world()
            .resource::<LandmarkRig>()
            .parts(LandmarkKind::Lava)
            .next()
            .expect("pièce");
        let toward = app
            .world()
            .get::<GlobalTransform>(facade_part)
            .expect("pièce")
            .rotation()
            * Vec3::Z;
        assert!(toward.x * s < -0.99, "la façade regarde l'axe : {toward:?}");
    }
}

#[test]
fn l_evenement_landmark_est_inscrit_et_chaque_immeuble_sort_comme_sujet_puis_se_range() {
    let mut app = app();
    drive(&mut app, 3);
    assert!(
        app.world().resource::<EventRegistry>().contains(EventName::Landmark),
        "l'événement `landmark` est inscrit par le plugin"
    );
    let handle = app.world().resource::<LandmarkHandle>().clone();
    for kind in LandmarkKind::ALL {
        let mut seen_subject = None;
        let mut obstruction = false;
        let mut was_active = false;
        let mut finished = false;
        // Apparition à 400 m + demi-largeur, rangement 160 m après : au plus 800 m à 60 m/s, avec de la marge.
        for frame in 0..(60 * 60) {
            if !was_active && frame % 120 == 0 {
                // Un ordre tombé près d'une arche-immeuble de la route est écarté par le directeur (« on retente plus tard ») :
                // on le redonne jusqu'à ce que l'immeuble apparaisse.
                handle.force(kind, None);
                app.world_mut()
                    .resource_mut::<TrafficDirector>()
                    .force(EventName::Landmark);
            }
            drive(&mut app, 1);
            let active = handle.lock().active().map(|a| a.kind);
            if active == Some(kind) {
                was_active = true;
                let director = app.world().resource::<TrafficDirector>();
                director.collect_subjects(|s| {
                    if s.kind == SubjectKind::Landmark {
                        seen_subject = Some((s.position, s.size));
                    }
                });
                obstruction |= !app.world().resource::<PilotTraffic>().obstructions.is_empty();
                // Aucune pose invalide.
                let stage = handle.lock();
                for paint in stage.paint(kind) {
                    assert!(paint.transform.translation.is_finite(), "{}", kind.as_str());
                    assert!(paint.transform.scale.is_finite() && paint.transform.rotation.is_finite());
                }
            } else if was_active {
                finished = true;
                break;
            }
        }
        assert!(was_active, "{} n'est jamais apparu", kind.as_str());
        assert!(finished, "{} n'a pas été rangé", kind.as_str());
        let (position, size) = seen_subject.unwrap_or_else(|| panic!("{} : pas de sujet `Landmark`", kind.as_str()));
        let span = handle.lock().landmark(kind).span();
        assert!(
            (size - span).abs() < span * 0.3 + 1.0,
            "{} : taille {size} pour une largeur de {span}",
            kind.as_str()
        );
        assert!(position.y > 0.0);
        let spans_road = handle.lock().landmark(kind).spans_road();
        assert_eq!(
            obstruction,
            spans_road,
            "{} : l'esquive n'est publiée que pour une structure qui enjambe la route",
            kind.as_str()
        );
        // La ville récupère sa place : plus aucune zone dégagée pour lui.
        drive(&mut app, 60);
    }
    // Les lumières sont éteintes une fois l'immeuble rangé.
    let field = app.world().resource::<CityField>();
    assert!(field.state.lights().iter().all(|l| l.color.length() == 0.0));
}
