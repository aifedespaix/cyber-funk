//! Création de la fenêtre opérateur et de sa caméra.

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::{WindowRef, WindowResizeConstraints, WindowResolution};
use bevy_egui::PrimaryEguiContext;

/// Marqueur de la fenêtre opérateur (la fenêtre principale reste le projecteur).
#[derive(Component, Debug, Default)]
pub struct OperatorWindow;

/// Titre de la fenêtre opérateur.
const TITLE: &str = "cyber-funk : opérateur";
/// Taille initiale en pixels logiques ; la fenêtre est redimensionnable.
const INITIAL_SIZE: (u32, u32) = (1280, 800);
/// Taille minimale : en dessous, les panneaux d'egui deviennent inutilisables.
const MIN_SIZE: (f32, f32) = (640.0, 400.0);

pub(crate) fn spawn_operator_window(mut commands: Commands) {
    let window = commands
        .spawn((
            OperatorWindow,
            Window {
                title: TITLE.into(),
                resolution: WindowResolution::new(INITIAL_SIZE.0, INITIAL_SIZE.1),
                resizable: true,
                resize_constraints: WindowResizeConstraints {
                    min_width: MIN_SIZE.0,
                    min_height: MIN_SIZE.1,
                    ..default()
                },
                ..default()
            },
        ))
        .id();
    // Une caméra 2D par fenêtre : celle-ci ne voit que l'interface egui, jamais la scène 3D du projecteur.
    commands.spawn((
        Camera2d,
        RenderTarget::Window(WindowRef::Entity(window)),
        PrimaryEguiContext,
    ));
}
