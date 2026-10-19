//! Scène de test : un cube qui tourne, vu par la caméra du projecteur.
//!
//! Sert à vérifier que le rendu va bien dans le projecteur et nulle part ailleurs. Provisoire : elle disparaît quand
//! le monde de Sillage s'affiche (J5).

use bevy::prelude::*;
use cf_render::CfSet;

/// Vitesse de rotation du cube, en rad/s (un tour en ~12 s : lent, lisible sur un projecteur).
const SPIN_SPEED_RAD_S: f32 = 0.5;

/// Cube de la scène de test.
#[derive(Component, Debug)]
struct TestCube;

/// Ajoute la scène de test.
#[derive(Debug, Default)]
pub struct TestScenePlugin;

impl Plugin for TestScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_scene)
            .add_systems(Update, spin_cube.in_set(CfSet::Simulate));
    }
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Sans cible explicite, une caméra 3D rend dans la fenêtre principale : le projecteur.
    commands.spawn((Camera3d::default(), Transform::from_xyz(2.5, 2.0, 3.5).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(3.0, 5.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        TestCube,
        Mesh3d(meshes.add(Cuboid::from_length(1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.9, 0.9),
            ..default()
        })),
        Transform::default(),
    ));
}

fn spin_cube(time: Res<Time>, mut cubes: Query<&mut Transform, With<TestCube>>) {
    for mut transform in &mut cubes {
        transform.rotate_y(SPIN_SPEED_RAD_S * time.delta_secs());
        transform.rotate_x(SPIN_SPEED_RAD_S * 0.5 * time.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;

    #[test]
    fn le_cube_tourne_au_fil_du_temps() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
            .add_systems(Update, spin_cube);
        let cube = app.world_mut().spawn((TestCube, Transform::default())).id();

        app.update(); // la première image n'a pas de delta
        app.update();
        app.update();

        let rotation = app.world().get::<Transform>(cube).unwrap().rotation;
        assert!(rotation.angle_between(Quat::IDENTITY) > 0.05, "{rotation:?}");
    }
}
