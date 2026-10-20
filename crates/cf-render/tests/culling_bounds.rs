//! « Aucun objet ne clignote en bord d'écran » : vérifié par la géométrie, pas à l'œil.
//!
//! Un objet clignote au bord de l'écran quand son `Aabb` est plus petit que son maillage : une partie de l'objet est
//! encore dans le champ de la caméra que Bevy le juge déjà dehors et ne le dessine plus. L'invariant qu'on tient donc
//! est : **tout point de la surface d'un maillage qui est dans le frustum de la caméra implique que l'entité est
//! visible** (`ViewVisibility`, calculée par le vrai système de Bevy). Les tests le vérifient sur des milliers de
//! poses de caméra tirées au hasard (graine fixe), sur la ville de boîtes entière (≈ 5 300 pièces), après
//! recyclage des tronçons, et sur un mesh qui suit la caméra. Chaque test a son témoin : avec un `Aabb` volontairement
//! trop petit, le même contrôle doit échouer, sinon il ne prouverait rien.
//!
//! Sans rendu : `MinimalPlugins` et le `CameraPlugin` de Bevy (frustum, visibilité), aucune carte graphique.

#![allow(clippy::expect_used, clippy::panic)]

use bevy::camera::primitives::{Aabb, Frustum, Sphere as BoundingSphere};
use bevy::camera::visibility::{NoAutoAabb, ViewVisibility};
use bevy::camera::Projection;
use bevy::math::{DVec3, Vec3A};
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use cf_render::streaming::{StreamedPiece, splitmix64};
use cf_render::{
    AbsolutePosition, BoxWorld, CorePlugin, StreamFocus, WorldPlugin, aabb_encloses_mesh, follow_camera_bounds,
};

/// Marge (m) dont un point doit être à l'intérieur du frustum pour compter : écarte les égalités à l'arrondi près.
const INSIDE_MARGIN_M: f32 = 1e-3;

/// Tirage déterministe dans [0, 1).
fn rand01(seed: u64, k: u64) -> f32 {
    (splitmix64(seed ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 40) as f32 / (1_u64 << 24) as f32
}

fn range(seed: u64, k: u64, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * rand01(seed, k)
}

fn app(world: BoxWorld) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        bevy::camera::CameraPlugin,
        CorePlugin,
        WorldPlugin::new(world),
    ))
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    // Lu par le calcul des `Aabb` des maillages à squelette, même quand il n'y en a pas.
    .init_asset::<SkinnedMeshInverseBindposes>();
    app
}

fn spawn_camera(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                aspect_ratio: 16.0 / 9.0,
                near: 0.1,
                far: 2_600.0,
                ..default()
            }),
            Transform::default(),
        ))
        .id()
}

/// Pose la caméra : position, lacet, tangage, champ vertical.
fn pose(app: &mut App, camera: Entity, seed: u64, around: Vec3, spread: Vec3) {
    let position = around
        + Vec3::new(
            range(seed, 1, -spread.x, spread.x),
            range(seed, 2, 0.0, spread.y),
            range(seed, 3, -spread.z, spread.z),
        );
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        range(seed, 4, 0.0, std::f32::consts::TAU),
        range(seed, 5, -0.6, 0.6),
        0.0,
    );
    let fov = range(seed, 6, 0.5, 1.7);
    let world = app.world_mut();
    if let Some(mut transform) = world.get_mut::<Transform>(camera) {
        *transform = Transform::from_translation(position).with_rotation(rotation);
    }
    if let Some(mut projection) = world.get_mut::<Projection>(camera)
        && let Projection::Perspective(perspective) = &mut *projection
    {
        perspective.fov = fov;
    }
}

/// Points de la surface d'un maillage : sommets et centres de triangles, en repère local.
fn surface_samples(mesh: &Mesh) -> Vec<Vec3> {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return Vec::new();
    };
    let vertices: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let mut samples = vertices.clone();
    let indices: Vec<usize> = match mesh.indices() {
        Some(Indices::U16(i)) => i.iter().map(|&i| usize::from(i)).collect(),
        Some(Indices::U32(i)) => i.iter().map(|&i| i as usize).collect(),
        None => (0..vertices.len()).collect(),
    };
    for tri in indices.chunks_exact(3) {
        samples.push((vertices[tri[0]] + vertices[tri[1]] + vertices[tri[2]]) / 3.0);
    }
    samples
}

fn point_in_frustum(frustum: &Frustum, point: Vec3) -> bool {
    // Rayon négatif = marge vers l'intérieur. `false` : sans le plan lointain, comme le culling de Bevy.
    let probe = BoundingSphere {
        center: Vec3A::from(point),
        radius: -INSIDE_MARGIN_M,
    };
    frustum.intersects_sphere(&probe, false)
}

/// Résultat d'un contrôle de clignotement.
#[derive(Debug, Default)]
struct Popping {
    /// Entités examinées (celles qui sont posées dans le monde, `Visibility` non cachée).
    checked: usize,
    /// Entités dont au moins un point de surface est dans le champ.
    in_view: usize,
    /// Entités dans le champ que Bevy juge pourtant invisibles : le clignotement.
    popping: usize,
}

/// Compare, pour chaque entité dessinable, « un point de son maillage est dans le champ » à `ViewVisibility`.
fn check_popping(app: &mut App, camera: Entity) -> Popping {
    let world = app.world_mut();
    let frustum = *world.get::<Frustum>(camera).expect("frustum de la caméra");
    let mut result = Popping::default();
    let mut query = world.query::<(&Mesh3d, &GlobalTransform, &ViewVisibility, &InheritedVisibility)>();
    let meshes = world.resource::<Assets<Mesh>>();
    for (mesh, transform, view, inherited) in query.iter(world) {
        if !inherited.get() {
            continue;
        }
        let Some(mesh) = meshes.get(&mesh.0) else { continue };
        result.checked += 1;
        let seen = surface_samples(mesh)
            .into_iter()
            .any(|p| point_in_frustum(&frustum, transform.transform_point(p)));
        if seen {
            result.in_view += 1;
            if !view.get() {
                result.popping += 1;
            }
        }
    }
    result
}

fn dense_city() -> BoxWorld {
    // 13 rangées par tronçon : ≈ 5 300 immeubles vivants. Sans coupe de brume : on teste la géométrie, pas la distance.
    BoxWorld::new(11).with_rows_per_chunk(13).with_fog_cull(None)
}

fn start_city(app: &mut App) {
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 1.0, 0.0))));
    app.update();
}

fn move_focus(app: &mut App, z: f64) {
    let world = app.world_mut();
    let mut query = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
    for mut p in query.iter_mut(world) {
        p.0.z = z;
    }
    app.update();
}

#[test]
fn les_aabb_de_la_ville_enveloppent_leurs_maillages() {
    let mut app = app(dense_city());
    start_city(&mut app);
    let world = app.world_mut();
    let mut query = world.query::<(&StreamedPiece, &Mesh3d, &Aabb)>();
    let meshes = world.resource::<Assets<Mesh>>();
    let mut checked = 0;
    for (_, mesh, aabb) in query.iter(world) {
        let mesh = meshes.get(&mesh.0).expect("mesh de la pièce");
        assert!(aabb_encloses_mesh(aabb, mesh, 0.0), "{aabb:?}");
        checked += 1;
    }
    // Toutes les pièces préallouées ont un `Aabb`, y compris celles qui sont cachées (pool).
    let pooled = app
        .world_mut()
        .query::<&StreamedPiece>()
        .iter(app.world())
        .count();
    assert_eq!(checked, pooled);
    assert!(pooled > 5_000, "{pooled} pièces");
}

#[test]
fn aucun_immeuble_ne_clignote_au_bord_de_l_ecran() {
    let mut app = app(dense_city());
    start_city(&mut app);
    let camera = spawn_camera(&mut app);
    let mut total = Popping::default();
    for seed in 0..120_u64 {
        // Dans la ville, au milieu des immeubles, de tous côtés et à toute hauteur.
        pose(
            &mut app,
            camera,
            seed,
            Vec3::new(0.0, 0.0, -150.0),
            Vec3::new(120.0, 120.0, 200.0),
        );
        app.update();
        let r = check_popping(&mut app, camera);
        assert_eq!(r.popping, 0, "pose {seed} : {r:?}");
        total.checked += r.checked;
        total.in_view += r.in_view;
    }
    eprintln!("clignotement : {total:?} sur 120 poses");
    // Le contrôle n'est pas vide : il y avait des immeubles dans le champ, et des pièces cachées du champ.
    assert!(total.in_view > 10_000, "{total:?}");
    assert!(total.checked > total.in_view, "{total:?}");
}

#[test]
fn les_aabb_restent_justes_apres_recyclage_des_troncons() {
    let mut app = app(dense_city());
    start_city(&mut app);
    let camera = spawn_camera(&mut app);
    // 5 km plus loin : toute la fenêtre a été recyclée, les pièces ont de nouvelles poses et de nouvelles échelles.
    let z = -5_000.0;
    move_focus(&mut app, z);
    for seed in 0..60_u64 {
        pose(
            &mut app,
            camera,
            1_000 + seed,
            Vec3::new(0.0, 0.0, z as f32 - 150.0),
            Vec3::new(120.0, 120.0, 200.0),
        );
        app.update();
        let r = check_popping(&mut app, camera);
        assert_eq!(r.popping, 0, "pose {seed} : {r:?}");
    }
}

#[test]
fn le_controle_a_des_dents_un_aabb_trop_petit_fait_clignoter() {
    let mut app = app(dense_city());
    start_city(&mut app);
    // Témoin : tous les immeubles reçoivent un `Aabb` de la moitié de leur taille (et `NoAutoAabb`, pour que Bevy ne
    // le corrige pas).
    let world = app.world_mut();
    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<StreamedPiece>>()
        .iter(world)
        .collect();
    for entity in entities {
        world.entity_mut(entity).insert((
            Aabb {
                center: Vec3A::ZERO,
                half_extents: Vec3A::splat(0.25),
            },
            NoAutoAabb,
        ));
    }
    let camera = spawn_camera(&mut app);
    let mut popping = 0;
    for seed in 0..40_u64 {
        pose(
            &mut app,
            camera,
            seed,
            Vec3::new(0.0, 0.0, -150.0),
            Vec3::new(120.0, 120.0, 200.0),
        );
        app.update();
        popping += check_popping(&mut app, camera).popping;
    }
    eprintln!("témoin : {popping} immeubles clignotants sur 40 poses");
    assert!(popping > 1_000, "le témoin devrait clignoter beaucoup, pas {popping} fois");
}

/// Un mesh minuscule dont le vertex shader écarte les sommets jusqu'à `REACH` m de l'origine de l'entité, laquelle suit
/// la caméra à distance fixe (la boîte d'air de la pluie, devant et à côté de l'objectif). Le maillage lu par le
/// processeur ne dit rien de cette déformation : on la simule par des points tirés dans le cube que le shader peut
/// atteindre.
const REACH: f32 = 60.0;
/// Décalage de l'entité par rapport à la caméra, en repère caméra : devant et à droite. Collée à l'objectif, elle
/// serait toujours dans le champ (le plan proche du frustum de Bevy 0.20 est à `near` mètres *derrière* l'objectif)
/// et le témoin ne prouverait rien.
const FOLLOW_OFFSET: Vec3 = Vec3::new(50.0, 0.0, -40.0);

fn follower_popping(with_bounds: bool) -> (usize, usize) {
    let mut app = app(BoxWorld::new(1));
    let camera = spawn_camera(&mut app);
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::from_length(0.1));
    let follower = app.world_mut().spawn((Mesh3d(mesh), Transform::default())).id();
    if with_bounds {
        app.world_mut().entity_mut(follower).insert(follow_camera_bounds(REACH));
    }
    let (mut seen_points, mut popping) = (0, 0);
    for seed in 0..200_u64 {
        pose(&mut app, camera, seed, Vec3::ZERO, Vec3::new(500.0, 100.0, 500.0));
        // Le suiveur est replacé d'après la caméra avant le calcul de visibilité de l'image.
        let eye = app.world().get::<Transform>(camera).expect("pose");
        let anchor = eye.translation + eye.rotation * FOLLOW_OFFSET;
        if let Some(mut t) = app.world_mut().get_mut::<Transform>(follower) {
            t.translation = anchor;
        }
        app.update();
        let frustum = *app.world().get::<Frustum>(camera).expect("frustum");
        let visible = app.world().get::<ViewVisibility>(follower).expect("visibilité").get();
        // Points que le shader peut atteindre : dans le cube de demi-arête REACH autour de l'entité.
        let any_in_view = (0..64_u64).any(|k| {
            let offset = Vec3::new(
                range(seed, 100 + k, -REACH, REACH),
                range(seed, 200 + k, -REACH, REACH),
                range(seed, 300 + k, -REACH, REACH),
            );
            point_in_frustum(&frustum, anchor + offset)
        });
        if any_in_view {
            seen_points += 1;
            if !visible {
                popping += 1;
            }
        }
    }
    (seen_points, popping)
}

#[test]
fn un_mesh_qui_suit_la_camera_n_est_jamais_coupe_avec_son_volume() {
    let (seen, popping) = follower_popping(true);
    assert!(seen > 150, "{seen} poses avec un point dans le champ");
    assert_eq!(popping, 0);
}

#[test]
fn sans_volume_le_suiveur_est_coupe_alors_qu_il_est_a_l_ecran() {
    // Témoin : avec le seul `Aabb` du mesh (10 cm), l'entité n'est jugée que sur son origine : dès qu'elle sort du champ
    // alors que le cube de la déformation y reste, elle disparaît.
    let (seen, popping) = follower_popping(false);
    assert!(seen > 150, "{seen}");
    assert!(popping > 20, "le témoin devrait clignoter, pas {popping} fois");
}

#[test]
fn le_volume_pose_a_la_main_survit_au_changement_de_mesh() {
    let mut app = app(BoxWorld::new(1));
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::from_length(0.1));
    let guarded = app
        .world_mut()
        .spawn((Mesh3d(mesh.clone()), follow_camera_bounds(REACH)))
        .id();
    let auto = app.world_mut().spawn(Mesh3d(mesh.clone())).id();
    app.update();
    // Le mesh change (rechargement d'asset) : Bevy recalcule l'`Aabb` des entités qui n'ont pas `NoAutoAabb`.
    if let Some(mut asset) = app.world_mut().resource_mut::<Assets<Mesh>>().get_mut(&mesh) {
        *asset = Mesh::from(Cuboid::from_length(0.2));
    }
    app.update();
    app.update();
    let guarded_aabb = *app.world().get::<Aabb>(guarded).expect("aabb posé");
    assert_eq!(guarded_aabb.half_extents, Vec3A::splat(REACH));
    let auto_aabb = *app.world().get::<Aabb>(auto).expect("aabb automatique");
    assert!((auto_aabb.half_extents.x - 0.1).abs() < 1e-6, "{auto_aabb:?}");
}
