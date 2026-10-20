//! Les deux compositeurs : celui de l'image (fond, monde, avant-plan), puis celui de la surcouche 2D.
//!
//! # Pourquoi deux compositeurs
//!
//! Le post-process (J4-02 : bloom, étalonnage, flash, tonemapping) s'équipe sur le compositeur, donc sur tout ce qu'il
//! dessine. Si la surcouche (cartons-titres, nom du DJ) passait par lui, elle serait étalonnée, vignettée, granulée,
//! aberrée et blanchie par les flashs comme le reste ; un titre blanc sur un néon prendrait aussi du bloom. Dans
//! Sillage, les titres étaient du DOM posé sur le canvas : jamais touchés par l'`EffectComposer`. On garde ce contrat.
//!
//! Le [`Compositor`] ne superpose donc que les trois couches 3D, et un second compositeur, l'[`OverlayCompositor`],
//! dessine ensuite le quad de la surcouche **par-dessus l'image déjà post-traitée**, sans effet :
//!
//! - même cible (la fenêtre), même format HDR et pas de MSAA : Bevy lui donne la **même texture principale** que le
//!   compositeur, qui contient alors l'image tonemappée ;
//! - `ClearColorConfig::None` : il ne l'efface pas, il dessine dessus ;
//! - ni bloom, ni étalonnage, ni tonemapping, ni dither.
//!
//! Le prix : une passe principale de plus (un seul quad) et une seconde recopie vers la fenêtre (la sortie du
//! compositeur est réécrite par celle du second). Surcouche éteinte (opacité nulle), sa caméra est désactivée et rien
//! de tout cela n'est payé.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, Camera3d, ClearColorConfig, Hdr, OrthographicProjection, Projection, ScalingMode};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use bevy::render::view::Msaa;

use super::{COMPOSITOR_INDEX, Layer, OVERLAY_COMPOSITOR_INDEX};

/// `Camera::order` du compositeur : après les quatre couches.
pub const COMPOSITOR_ORDER: isize = 10;
/// `Camera::order` du compositeur de la surcouche : après le compositeur (et donc après son post-process).
pub const OVERLAY_COMPOSITOR_ORDER: isize = 11;

/// La caméra qui compose les couches 3D (fond, monde, avant-plan) dans la fenêtre. C'est elle que le post-process
/// (J4-02) équipe : il s'applique ainsi à l'image composée, surcouche 2D exclue.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Compositor;

/// La caméra qui pose la surcouche 2D sur l'image post-traitée, sans aucun effet (voir la doc du module).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct OverlayCompositor;

/// Quad plein cadre d'un compositeur qui affiche la texture d'une couche.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct LayerQuad(pub(super) Layer);

/// Index de `RenderLayers` du compositeur qui affiche `layer`.
pub(super) const fn compositor_index(layer: Layer) -> usize {
    match layer {
        Layer::Overlay => OVERLAY_COMPOSITOR_INDEX,
        _ => COMPOSITOR_INDEX,
    }
}

/// Projection des compositeurs : orthographique 2×2, sans dépendre de la taille de la fenêtre.
fn quad_projection() -> Projection {
    Projection::Orthographic(OrthographicProjection {
        near: 0.1,
        far: 10.0,
        scaling_mode: ScalingMode::Fixed {
            width: 2.0,
            height: 2.0,
        },
        ..OrthographicProjection::default_3d()
    })
}

/// Fait naître les deux compositeurs et les quatre quads de couche.
pub(super) fn spawn_compositors(
    commands: &mut Commands,
    quad: &Handle<Mesh>,
    materials: &[Handle<StandardMaterial>; 4],
) {
    commands.spawn((
        Compositor,
        Camera3d::default(),
        Camera {
            order: COMPOSITOR_ORDER,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        quad_projection(),
        RenderLayers::layer(COMPOSITOR_INDEX),
        Hdr,
        // Ni tonemapping ni MSAA à la naissance : les quads sont des aplats alignés sur les pixels. Le post-process
        // (`PostFxPlugin`) pose ici bloom, étalonnage et tonemapping ; sans lui, l'image sort brute.
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        Transform::default(),
    ));
    commands.spawn((
        OverlayCompositor,
        Camera3d::default(),
        Camera {
            order: OVERLAY_COMPOSITOR_ORDER,
            // Dessine sur l'image du compositeur, qu'il partage (même cible, même format, même MSAA).
            clear_color: ClearColorConfig::None,
            // Allumé avec la surcouche (`apply_opacity`).
            is_active: false,
            ..default()
        },
        quad_projection(),
        RenderLayers::layer(OVERLAY_COMPOSITOR_INDEX),
        // `Hdr` et `Msaa::Off` comme le compositeur : c'est la condition pour partager sa texture principale.
        Hdr,
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        Transform::default(),
    ));
    for layer in Layer::ALL {
        commands.spawn((
            LayerQuad(layer),
            Mesh3d(quad.clone()),
            MeshMaterial3d(materials[layer.index()].clone()),
            // Du fond (le plus loin) à l'avant-plan (le plus près) : le tri des matériaux transparents fait le reste.
            Transform::from_xyz(0.0, 0.0, -(4.0 - layer.index() as f32)),
            RenderLayers::layer(compositor_index(layer)),
            Visibility::Hidden,
        ));
    }
}
