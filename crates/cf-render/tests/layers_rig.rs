//! Le rig de couches sans rendu : une `App` avec `MinimalPlugins`, sans GPU ni fenêtre réelle.
//!
//! Ce que ces tests tiennent : la structure du rig (caméras, cibles, ordres, `RenderLayers`), l'extinction des couches
//! à 0 (le « coût GPU nul » se vérifie par `Camera::is_active`, pas par un chrono), l'application de la pose et de
//! l'origine flottante, le suivi de la fenêtre, et l'absence d'écriture en régime établi. Ce que la carte graphique en
//! fait (composition, opacité à l'écran, avant-plan devant un mur) est vérifié par `examples/layers_demo.rs`.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, Projection, RenderTarget};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};
use cf_render::{
    CameraPose, CfSet, Compositor, CorePlugin, FloatingOrigin, Layer, LayerCamera, LayerOpacity, LayerTargets,
    LayersPlugin, OFF_BELOW,
};

const SIZE: UVec2 = UVec2::new(64, 36);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), CorePlugin))
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_resource::<FloatingOrigin>()
        .add_plugins(LayersPlugin { fallback_size: SIZE });
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

fn is_active(app: &mut App, layer: Layer) -> bool {
    let entity = camera_of(app, layer);
    app.world().get::<Camera>(entity).expect("Camera").is_active
}

fn quad_visibility(app: &mut App, layer: Layer) -> Visibility {
    let material = app.world().resource::<LayerTargets>().image(layer).clone();
    // Le quad de la couche est celui dont le matériau porte la texture de la couche.
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let handle = materials
        .iter()
        .find(|(_, m)| m.base_color_texture.as_ref() == Some(&material))
        .map(|(id, _)| id)
        .expect("matériau de couche");
    app.world_mut()
        .query::<(&MeshMaterial3d<StandardMaterial>, &Visibility)>()
        .iter(app.world())
        .find(|(m, _)| m.0.id() == handle)
        .map(|(_, v)| *v)
        .expect("quad de couche")
}

fn material_alpha(app: &mut App, layer: Layer) -> f32 {
    let image = app.world().resource::<LayerTargets>().image(layer).clone();
    app.world()
        .resource::<Assets<StandardMaterial>>()
        .iter()
        .find(|(_, m)| m.base_color_texture.as_ref() == Some(&image))
        .map(|(_, m)| m.base_color.to_linear().alpha)
        .expect("matériau de couche")
}

#[test]
fn le_rig_a_quatre_cameras_de_couche_et_un_compositeur() {
    let mut app = app();
    app.update();

    let mut orders = Vec::new();
    let mut targets = Vec::new();
    for layer in Layer::ALL {
        let entity = camera_of(&mut app, layer);
        let world = app.world();
        orders.push(world.get::<Camera>(entity).unwrap().order);
        assert_eq!(
            world.get::<RenderLayers>(entity),
            Some(&layer.render_layers()),
            "{layer:?}"
        );
        let RenderTarget::Image(target) = world.get::<RenderTarget>(entity).unwrap() else {
            panic!("{layer:?} doit rendre dans une texture");
        };
        targets.push(target.handle.clone());
    }
    // Rendues de l'arrière vers l'avant, chacune dans sa texture (donc avec sa profondeur).
    assert_eq!(orders, [0, 1, 2, 3]);
    for (i, a) in targets.iter().enumerate() {
        for b in &targets[i + 1..] {
            assert_ne!(
                a, b,
                "deux couches partagent une texture : elles partageraient aussi la profondeur"
            );
        }
    }

    // Un seul compositeur, qui rend dans la fenêtre après toutes les couches.
    let compositors: Vec<_> = app
        .world_mut()
        .query_filtered::<&Camera, With<Compositor>>()
        .iter(app.world())
        .collect();
    assert_eq!(compositors.len(), 1);
    assert!(compositors[0].order > Layer::Overlay.order());
    let entity = app
        .world_mut()
        .query_filtered::<Entity, With<Compositor>>()
        .single(app.world())
        .unwrap();
    assert!(matches!(
        app.world().get::<RenderTarget>(entity),
        Some(RenderTarget::Window(_))
    ));
}

#[test]
fn une_couche_a_zero_a_sa_camera_desactivee_et_son_quad_masque() {
    let mut app = app();
    app.update();
    for layer in Layer::ALL {
        assert!(is_active(&mut app, layer), "{layer:?} allumée par défaut");
        assert_eq!(quad_visibility(&mut app, layer), Visibility::Inherited);
    }

    app.world_mut().resource_mut::<LayerOpacity>().set(Layer::World, 0.0);
    app.world_mut()
        .resource_mut::<LayerOpacity>()
        .set(Layer::Foreground, 0.3);
    app.update();
    assert!(!is_active(&mut app, Layer::World));
    assert_eq!(quad_visibility(&mut app, Layer::World), Visibility::Hidden);
    // 0,3 est visible : la couche rend, composée à 30 %.
    assert!(is_active(&mut app, Layer::Foreground));
    assert!((material_alpha(&mut app, Layer::Foreground) - 0.3).abs() < 1e-6);
    // Les autres ne bougent pas.
    assert!(is_active(&mut app, Layer::Background));
    assert!(is_active(&mut app, Layer::Overlay));

    // Rallumée : elle redevient active.
    app.world_mut().resource_mut::<LayerOpacity>().set(Layer::World, 1.0);
    app.update();
    assert!(is_active(&mut app, Layer::World));
    assert_eq!(quad_visibility(&mut app, Layer::World), Visibility::Inherited);
}

#[test]
fn le_fondu_garde_la_couche_active_jusqu_au_seuil_puis_l_eteint() {
    let mut app = app();
    let steps = 40;
    let mut off_at = None;
    for step in 0..=steps {
        let value = 1.0 - step as f32 / steps as f32;
        app.world_mut().resource_mut::<LayerOpacity>().set(Layer::World, value);
        app.update();
        let active = is_active(&mut app, Layer::World);
        // Actif exactement tant que l'opacité dépasse le seuil : jamais d'extinction avant la fin visible du fondu.
        assert_eq!(active, value > OFF_BELOW, "opacité {value}");
        if !active && off_at.is_none() {
            off_at = Some(step);
        }
        if active {
            assert!((material_alpha(&mut app, Layer::World) - value).abs() < 1e-6);
        }
    }
    assert_eq!(off_at, Some(steps), "la couche ne s'éteint qu'à la dernière marche");
}

#[test]
fn la_pose_est_appliquee_aux_cameras_et_le_monde_suit_l_origine_flottante() {
    let mut app = app();
    // L'origine saute près du point suivi : 400 km du départ.
    app.world_mut()
        .resource_mut::<FloatingOrigin>()
        .recenter(DVec3::new(0.0, 0.0, -403_200.0));
    let pose = CameraPose {
        position: DVec3::new(10.0, 2.0, -400_000.25),
        rotation: Quat::from_rotation_y(0.7),
        fov_deg: 50.0,
        cabin: false,
        sky_drift: Vec3::new(1.0, 2.0, 3.0),
    };
    app.insert_resource(pose);
    app.update();

    let world = camera_of(&mut app, Layer::World);
    let transform = *app.world().get::<Transform>(world).unwrap();
    assert!(
        transform.translation.distance(Vec3::new(10.0, 2.0, 3_199.75)) < 1e-3,
        "{transform:?}"
    );
    assert!(transform.rotation.angle_between(pose.rotation) < 1e-5);

    let background = camera_of(&mut app, Layer::Background);
    let transform = *app.world().get::<Transform>(background).unwrap();
    assert_eq!(transform.translation, pose.sky_drift);

    // L'avant-plan est en repère caméra : à l'origine, sans rotation, quelle que soit la pose.
    let foreground = camera_of(&mut app, Layer::Foreground);
    let transform = *app.world().get::<Transform>(foreground).unwrap();
    assert_eq!(transform.translation, Vec3::ZERO);
    assert_eq!(transform.rotation, Quat::IDENTITY);

    // Le FOV est commun aux trois caméras 3D.
    for layer in [Layer::Background, Layer::World, Layer::Foreground] {
        let entity = camera_of(&mut app, layer);
        let Projection::Perspective(perspective) = app.world().get::<Projection>(entity).unwrap() else {
            panic!("{layer:?} doit être en perspective");
        };
        assert!((perspective.fov - 50.0_f32.to_radians()).abs() < 1e-6, "{layer:?}");
    }
}

#[test]
fn les_textures_suivent_la_fenetre() {
    let mut app = app();
    let window = app
        .world_mut()
        .spawn((
            Window {
                resolution: WindowResolution::new(160, 90),
                ..default()
            },
            PrimaryWindow,
        ))
        .id();
    app.update();
    assert_eq!(app.world().resource::<LayerTargets>().size(), UVec2::new(160, 90));

    app.world_mut().get_mut::<Window>(window).unwrap().resolution = WindowResolution::new(320, 180);
    app.update();
    let targets = app.world().resource::<LayerTargets>();
    assert_eq!(targets.size(), UVec2::new(320, 180));
    let images = app.world().resource::<Assets<Image>>();
    for layer in Layer::ALL {
        assert_eq!(
            images.get(targets.image(layer)).unwrap().size(),
            UVec2::new(320, 180),
            "{layer:?}"
        );
    }
}

/// Nombre d'entités du rig dont un composant a été marqué modifié pendant l'image.
#[derive(Resource, Default)]
struct Writes(usize);

#[test]
fn en_regime_etabli_le_rig_n_ecrit_rien() {
    // Un système de `Last` voit ce que les systèmes de l'image ont écrit (la détection de changement d'un système
    // compte depuis son propre dernier passage, donc exactement depuis l'image précédente).
    fn watch(
        changed: Query<
            (),
            Or<(
                (Changed<Camera>, With<LayerCamera>),
                (Changed<Transform>, With<LayerCamera>),
                (Changed<Projection>, With<LayerCamera>),
                (Changed<Visibility>, With<Mesh3d>),
            )>,
        >,
        mut writes: ResMut<Writes>,
    ) {
        writes.0 = changed.iter().count();
    }
    let mut app = app();
    app.init_resource::<Writes>().add_systems(Last, watch);
    app.update();
    assert!(
        app.world().resource::<Writes>().0 > 0,
        "la première image configure le rig : le détecteur doit le voir"
    );
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Writes>().0,
        0,
        "le rig réécrit des composants inchangés"
    );

    // Et il se remet à écrire quand l'entrée change : le détecteur n'est pas aveugle.
    app.world_mut().resource_mut::<LayerOpacity>().set(Layer::World, 0.0);
    app.update();
    assert!(app.world().resource::<Writes>().0 > 0);
    app.update();
    assert_eq!(app.world().resource::<Writes>().0, 0);
}

#[test]
fn les_systemes_du_rig_sont_dans_present() {
    // Garde-fou d'ordonnancement : le rig lit la pose écrite en `Direct`. Une pose écrite dans `Direct` est donc
    // appliquée la même image.
    fn direct(mut pose: ResMut<CameraPose>) {
        pose.fov_deg = 33.0;
    }
    let mut app = app();
    app.add_systems(Update, direct.in_set(CfSet::Direct));
    app.update();
    let entity = camera_of(&mut app, Layer::World);
    let Projection::Perspective(perspective) = app.world().get::<Projection>(entity).unwrap() else {
        panic!("perspective attendue");
    };
    assert!((perspective.fov - 33.0_f32.to_radians()).abs() < 1e-6);
}
