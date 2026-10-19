//! cyber-funk : visuels 3D temps réel pour soirées DJ.
//!
//! Le binaire ne fait qu'assembler les plugins. Les fenêtres projecteur et opérateur, la ligne de commande et les
//! journaux arrivent avec la tâche J0-02.

use bevy::prelude::*;
use cf_cinema::CinemaPlugin;
use cf_operator::OperatorPlugin;
use cf_render::CorePlugin;
use cf_world_neon::NeonWorldPlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "cyber-funk".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((CorePlugin, CinemaPlugin, NeonWorldPlugin, OperatorPlugin))
        .run()
}
