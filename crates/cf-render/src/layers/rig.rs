//! Le rig : quatre caméras de couche, leurs textures, et le compositeur qui les superpose.
//!
//! Voir la doc du module parent pour le pourquoi. Ici : le comment.
//!
//! Système par système, en `CfSet::Present` :
//! 1. [`resize_targets`] garde les textures à la taille de la fenêtre ;
//! 2. [`apply_pose`] pose les caméras d'après [`CameraPose`] (position, orientation, FOV) ;
//! 3. [`apply_opacity`] allume ou éteint les couches d'après [`LayerOpacity`] et règle l'opacité des quads ; la caméra
//!    de l'[`OverlayCompositor`] suit la surcouche.
//!
//! Aucun ne fait d'allocation en régime établi : les écritures sont conditionnelles (`set_if_neq`, comparaison avec la
//! valeur déjà appliquée) pour ne pas déclencher de ré-extraction de composants inchangés.

use bevy::asset::RenderAssetUsages;
use bevy::camera::{
    Camera, Camera2d, Camera3d, ClearColorConfig, Hdr, PerspectiveProjection, Projection, RenderTarget,
};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureFormat};
use bevy::render::view::Msaa;
use bevy::window::PrimaryWindow;

use super::compositor::{LayerQuad, OverlayCompositor, spawn_compositors};
use super::{Layer, LayerOpacity, LayerWarmup};
use crate::{CameraPose, CfSet, FloatingOrigin};

/// Format des textures de couche : HDR (le bloom garde les valeurs au-delà de 1) avec un canal alpha de couverture.
const LAYER_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
/// Taille des textures tant qu'aucune fenêtre n'est connue (tests, démarrage).
const FALLBACK_SIZE: UVec2 = UVec2::new(1280, 720);

/// Plans de coupe `(proche, lointain)` de chaque couche, en m.
///
/// Le monde reprend `FAR_CINEMA` de Sillage (2 600 m, `cameras.ts`) ; le fond couvre la voûte (rayon 1 000 m) décalée
/// de `sky_drift` ; l'avant-plan descend à 2 cm pour l'habitacle et les particules collées à l'objectif.
const fn clip_planes(layer: Layer) -> (f32, f32) {
    match layer {
        Layer::Background => (1.0, 20_000.0),
        Layer::World => (0.1, 2_600.0),
        Layer::Foreground => (0.02, 50.0),
        // La caméra 2D a son propre volume.
        Layer::Overlay => (-1_000.0, 1_000.0),
    }
}

/// Couleur d'effacement d'une couche : le fond est opaque (la pile repose sur lui), le reste transparent.
fn clear_color(layer: Layer) -> Color {
    match layer {
        Layer::Background => Color::BLACK,
        _ => Color::NONE,
    }
}

/// Caméra d'une couche. Une par [`Layer`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerCamera(pub Layer);

/// Les textures de couche, et leur taille courante.
#[derive(Resource, Debug)]
pub struct LayerTargets {
    images: [Handle<Image>; 4],
    materials: [Handle<StandardMaterial>; 4],
    size: UVec2,
    /// Dernière opacité écrite dans le matériau de chaque couche (`NaN` : jamais écrite).
    applied: [f32; 4],
}

impl LayerTargets {
    /// Texture dans laquelle la caméra de `layer` rend.
    #[must_use]
    pub fn image(&self, layer: Layer) -> &Handle<Image> {
        &self.images[layer.index()]
    }

    /// Taille courante des textures, en pixels.
    #[must_use]
    pub fn size(&self) -> UVec2 {
        self.size
    }
}

/// Installe le rig de couches : ressources [`CameraPose`] et [`LayerOpacity`], les six caméras (quatre couches, deux
/// compositeurs) et leurs quads.
///
/// Les caméras naissent à `Startup`. Tant que `CfSet::Present` n'a pas tourné, toutes les couches sont éteintes.
#[derive(Debug, Clone, Copy)]
pub struct LayersPlugin {
    /// Taille des textures à la création, si la fenêtre principale n'existe pas encore (elles suivent ensuite la
    /// fenêtre).
    pub fallback_size: UVec2,
}

impl Default for LayersPlugin {
    fn default() -> Self {
        Self {
            fallback_size: FALLBACK_SIZE,
        }
    }
}

impl Plugin for LayersPlugin {
    fn build(&self, app: &mut App) {
        let fallback = self.fallback_size.max(UVec2::ONE);
        app.init_resource::<CameraPose>()
            .init_resource::<LayerOpacity>()
            .add_systems(
                Startup,
                move |commands: Commands,
                      images: ResMut<Assets<Image>>,
                      meshes: ResMut<Assets<Mesh>>,
                      materials: ResMut<Assets<StandardMaterial>>,
                      window: Query<&Window, With<PrimaryWindow>>| {
                    let size = window.single().map_or(fallback, |w| w.physical_size().max(UVec2::ONE));
                    spawn_rig(commands, images, meshes, materials, size);
                },
            )
            .add_systems(
                Update,
                (resize_targets, apply_pose, apply_opacity)
                    .chain()
                    .in_set(CfSet::Present),
            );
    }
}

fn target_image(size: UVec2) -> Image {
    let mut image = Image::new_target_texture(size.x, size.y, LAYER_FORMAT, None);
    // Effacée à chaque image par sa caméra : inutile d'en garder une copie de zéros côté processeur.
    image.data = None;
    // Rien à recopier : sans cela, Bevy cherche une ancienne texture et avertit à la création.
    image.copy_on_resize = false;
    image.asset_usage = RenderAssetUsages::default();
    image
}

fn spawn_rig(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    size: UVec2,
) {
    let image_handles = Layer::ALL.map(|_| images.add(target_image(size)));
    // Quad plein cadre : le compositeur est orthographique 2×2, sans dépendre de la taille de la fenêtre.
    let quad = meshes.add(Rectangle::new(2.0, 2.0));
    // Matériau « premultiplied » : la texture de couche sort du MSAA avec des couleurs déjà multipliées par la
    // couverture (alpha). Le mode `Blend` les multiplierait une seconde fois et cernerait de sombre chaque silhouette.
    // Couleur de base (o, o, o, o) : l'opacité o s'applique aux couleurs comme à l'alpha.
    let material_handles = Layer::ALL.map(|layer| {
        materials.add(StandardMaterial {
            base_color: Color::NONE,
            base_color_texture: Some(image_handles[layer.index()].clone()),
            unlit: true,
            alpha_mode: AlphaMode::Premultiplied,
            fog_enabled: false,
            ..default()
        })
    });

    for layer in Layer::ALL {
        let target = RenderTarget::from(image_handles[layer.index()].clone());
        let camera = Camera {
            order: layer.order(),
            clear_color: ClearColorConfig::Custom(clear_color(layer)),
            // Éteinte jusqu'au premier `apply_opacity` : pas de rendu avant que le look ait parlé.
            is_active: false,
            ..default()
        };
        let (near, far) = clip_planes(layer);
        let base = (
            LayerCamera(layer),
            camera,
            target,
            layer.render_layers(),
            Hdr,
            Transform::default(),
        );
        // `Camera3d` / `Camera2d` dans le lot de création : un `Camera` sans graphe de rendu à sa naissance fait
        // avertir Bevy.
        if layer == Layer::Overlay {
            commands.spawn((Camera2d, Msaa::Off, base));
        } else {
            commands.spawn((
                Camera3d::default(),
                Projection::Perspective(PerspectiveProjection { near, far, ..default() }),
                // Ni tonemapping ni dither : la couche est un intermédiaire HDR, l'image finale est mise en forme par
                // le compositeur (J4-02).
                Tonemapping::None,
                DebandDither::Disabled,
                Msaa::Sample4,
                base,
            ));
        }
    }

    spawn_compositors(&mut commands, &quad, &material_handles);

    commands.insert_resource(LayerTargets {
        images: image_handles,
        materials: material_handles,
        size,
        applied: [f32::NAN; 4],
    });
}

/// Garde les textures de couche à la taille (physique) de la fenêtre principale.
fn resize_targets(
    mut targets: ResMut<LayerTargets>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok(window) = window.single() else { return };
    let size = window.physical_size();
    // Fenêtre réduite : on garde les textures telles quelles plutôt que d'en créer de vides.
    if size.x == 0 || size.y == 0 || size == targets.size {
        return;
    }
    for handle in &targets.images {
        if let Some(mut image) = images.get_mut(handle) {
            image.resize(Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            });
        }
    }
    targets.size = size;
}

/// Pose les caméras de couche d'après la pose du réalisateur.
fn apply_pose(
    pose: Res<CameraPose>,
    origin: Option<Res<FloatingOrigin>>,
    mut cameras: Query<(&LayerCamera, &mut Transform, &mut Projection)>,
) {
    let origin = origin.map_or(DVec3::ZERO, |origin| origin.origin());
    let fov = pose.fov_rad();
    for (LayerCamera(layer), mut transform, mut projection) in &mut cameras {
        let Some(view) = pose.view_for(*layer, origin) else {
            continue;
        };
        transform.set_if_neq(Transform {
            translation: view.translation,
            rotation: view.rotation,
            scale: Vec3::ONE,
        });
        // Lecture d'abord : `&mut *projection` marquerait la projection modifiée même sans écriture.
        let stale = matches!(&*projection, Projection::Perspective(p) if (p.fov - fov).abs() > f32::EPSILON);
        if stale && let Projection::Perspective(perspective) = &mut *projection {
            perspective.fov = fov;
        }
    }
}

/// Allume ou éteint chaque couche, et règle l'opacité de son quad.
///
/// Une couche éteinte n'a ni caméra active ni quad visible : elle ne coûte rien au GPU.
fn apply_opacity(
    requested: Res<LayerOpacity>,
    warmup: Option<Res<LayerWarmup>>,
    mut targets: ResMut<LayerTargets>,
    mut cameras: Query<(&LayerCamera, &mut Camera), Without<OverlayCompositor>>,
    mut overlay_compositor: Query<&mut Camera, (With<OverlayCompositor>, Without<LayerCamera>)>,
    mut quads: Query<(&LayerQuad, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Préchauffe : tout est allumé à pleine opacité (voir `LayerWarmup`).
    let opacity = if warmup.is_some_and(|warmup| warmup.0) {
        LayerOpacity::default()
    } else {
        *requested
    };
    for (LayerCamera(layer), mut camera) in &mut cameras {
        let on = opacity.is_on(*layer);
        if camera.is_active != on {
            camera.is_active = on;
        }
    }
    // Le second compositeur n'a que la surcouche à poser : éteint avec elle.
    let overlay_on = opacity.is_on(Layer::Overlay);
    for mut camera in &mut overlay_compositor {
        if camera.is_active != overlay_on {
            camera.is_active = overlay_on;
        }
    }
    for (LayerQuad(layer), mut visibility) in &mut quads {
        let wanted = if opacity.is_on(*layer) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
    for layer in Layer::ALL {
        let value = opacity.get(layer);
        let index = layer.index();
        // `NaN != x` : la première image écrit toujours.
        if targets.applied[index].to_bits() == value.to_bits() {
            continue;
        }
        targets.applied[index] = value;
        if let Some(mut material) = materials.get_mut(&targets.materials[index]) {
            material.base_color = Color::linear_rgba(value, value, value, value);
        }
    }
}
