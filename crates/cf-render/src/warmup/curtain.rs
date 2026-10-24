//! Le rideau : un aplat noir par-dessus tout le projecteur pendant le chargement.
//!
//! La préchauffe fait **vraiment** rendre toutes les couches (et leurs copies de préchauffe) : c'est ce qui compile les
//! pipelines exacts du show. Le public ne doit pas voir ce rendu de travail ; le rideau le cache. C'est une caméra
//! ordinaire, la dernière de la pile, qui dessine un quad noir opaque plein cadre : elle part au passage à `Ready`.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, Camera3d, ClearColorConfig, Hdr, OrthographicProjection, Projection, ScalingMode};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use bevy::render::view::Msaa;

/// `Camera::order` du rideau : après le compositeur (10) et celui de la surcouche (11).
pub const CURTAIN_ORDER: isize = 100;
/// Index de `RenderLayers` du quad du rideau (0 à 5 sont pris par les couches et les compositeurs).
const CURTAIN_LAYER: usize = 7;

/// Marqueur des entités du rideau (caméra et quad).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Curtain;

/// Fait naître le rideau (`Startup`). Sans `Assets<Mesh>` (tests de pure logique) il n'y a rien à dessiner : on passe.
pub(super) fn spawn_curtain(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    commands.spawn((
        Curtain,
        Name::new("rideau (caméra)"),
        Camera3d::default(),
        Camera {
            order: CURTAIN_ORDER,
            // Le quad couvre tout : rien à effacer, et ne pas effacer préserve la texture partagée du compositeur.
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            near: 0.1,
            far: 10.0,
            scaling_mode: ScalingMode::Fixed {
                width: 2.0,
                height: 2.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        RenderLayers::layer(CURTAIN_LAYER),
        // Mêmes réglages que le compositeur de la surcouche : même texture principale, aucun effet.
        Hdr,
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        Transform::default(),
    ));
    commands.spawn((
        Curtain,
        Name::new("rideau (aplat)"),
        Mesh3d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            unlit: true,
            fog_enabled: false,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, -5.0),
        RenderLayers::layer(CURTAIN_LAYER),
    ));
}

/// `OnEnter(Ready)` : le rideau tombe. La caméra s'éteint (plus aucune passe) et l'aplat se cache.
pub(super) fn lift_curtain(
    mut cameras: Query<&mut Camera, With<Curtain>>,
    mut quads: Query<&mut Visibility, With<Curtain>>,
) {
    for mut camera in &mut cameras {
        camera.is_active = false;
    }
    for mut visibility in &mut quads {
        *visibility = Visibility::Hidden;
    }
}
