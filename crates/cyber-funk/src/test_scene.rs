//! Scène de test : un cube qui tourne, vu par la caméra du projecteur.
//!
//! Sert à vérifier que le rendu va bien dans le projecteur et nulle part ailleurs, et (J1-06) que l'audio arrive
//! jusqu'à l'image : le cube pulse sur chaque kick et prend la couleur de la section du morceau (bleu calme, cyan
//! groove, rouge sommet). Provisoire : elle disparaît quand
//! le monde de Sillage s'affiche (J5).

use bevy::prelude::*;
use cf_render::audio::Section;
use cf_render::{AudioFeatures, CfSet, Kick};

/// Vitesse de rotation du cube, en rad/s (un tour en ~12 s : lent, lisible sur un projecteur).
const SPIN_SPEED_RAD_S: f32 = 0.5;

/// Grossissement du cube au plus fort d'un kick (0,45 = +45 %).
const PULSE_GAIN: f32 = 0.45;
/// Constante de temps de la retombée de l'impulsion (s) : le cube a retrouvé sa taille avant le kick suivant à 128 BPM.
const PULSE_DECAY_S: f32 = 0.12;
/// Vitesse à laquelle la couleur rejoint celle de la section (par seconde).
const COLOR_RATE: f32 = 6.0;

/// Humeur du cube : impulsion du dernier kick et couleur courante.
#[derive(Resource, Debug)]
struct CubeMood {
    /// 1 au kick, retombe vers 0.
    pulse: f32,
    /// Couleur courante (sRGB), en route vers celle de la section.
    color: [f32; 3],
    /// Dernier état écrit dans le matériau, pour ne pas le toucher quand rien ne bouge.
    applied: Option<(f32, [f32; 3])>,
}

impl Default for CubeMood {
    fn default() -> Self {
        Self {
            pulse: 0.0,
            color: section_color(Section::Groove),
            applied: None,
        }
    }
}

/// Couleur du cube pour une section (sRGB).
fn section_color(section: Section) -> [f32; 3] {
    match section {
        Section::Calm => [0.1, 0.3, 0.95],
        Section::Groove => [0.1, 0.9, 0.9],
        Section::Peak => [1.0, 0.15, 0.45],
    }
}

/// Fait avancer l'impulsion de `dt` secondes.
fn decay(pulse: f32, dt: f32) -> f32 {
    pulse * (-dt / PULSE_DECAY_S).exp()
}

/// Fait avancer une couleur vers sa cible de `dt` secondes.
fn approach(color: [f32; 3], target: [f32; 3], dt: f32) -> [f32; 3] {
    let k = 1.0 - (-COLOR_RATE * dt).exp();
    [0, 1, 2].map(|i| color[i] + (target[i] - color[i]) * k)
}

/// Cube de la scène de test.
#[derive(Component, Debug)]
struct TestCube;

/// Ajoute la scène de test.
#[derive(Debug, Default)]
pub struct TestScenePlugin;

impl Plugin for TestScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CubeMood>()
            .add_systems(Startup, spawn_scene)
            .add_systems(
                Update,
                (spin_cube, react_to_audio, apply_mood).chain().in_set(CfSet::Simulate),
            );
    }
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Sans cible explicite, une caméra 3D rend dans la fenêtre principale : le projecteur.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(2.5, 2.0, 3.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
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

/// Un kick relance l'impulsion ; la couleur suit la section de l'état d'analyse (déjà décalé du retard visuel). La
/// section vient de l'état et non du message `SectionChanged` : un changement de source remet la section à `groove`
/// sans message, et le cube doit suivre.
fn react_to_audio(
    time: Res<Time>,
    features: Res<AudioFeatures>,
    mut kicks: MessageReader<Kick>,
    mut mood: ResMut<CubeMood>,
) {
    let dt = time.delta_secs();
    mood.pulse = decay(mood.pulse, dt);
    if kicks.read().count() > 0 {
        mood.pulse = 1.0;
    }
    mood.color = approach(mood.color, section_color(features.section), dt);
}

/// Écrit l'humeur dans le cube : taille et couleur (le matériau n'est touché que si elle a bougé).
fn apply_mood(
    mut mood: ResMut<CubeMood>,
    mut cubes: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>), With<TestCube>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let state = (mood.pulse, mood.color);
    let scale = 1.0 + PULSE_GAIN * mood.pulse;
    let changed = mood.applied.is_none_or(|(pulse, color)| {
        (pulse - state.0).abs() > 1e-3 || color.iter().zip(state.1).any(|(a, b)| (a - b).abs() > 1e-3)
    });
    for (mut transform, handle) in &mut cubes {
        transform.scale = Vec3::splat(scale);
        if changed && let Some(mut material) = materials.get_mut(&handle.0) {
            let [r, g, b] = state.1;
            material.base_color = Color::srgb(r, g, b);
            // Le kick fait aussi briller le cube : la couleur de la section, poussée par l'impulsion.
            material.emissive = LinearRgba::rgb(r, g, b) * (mood.pulse * 2.0);
        }
    }
    if changed {
        mood.applied = Some(state);
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

    #[test]
    fn l_impulsion_retombe_et_la_couleur_rejoint_la_section() {
        assert!(decay(1.0, 0.0) > 0.99);
        assert!(decay(1.0, 0.5) < 0.02);
        let mut color = section_color(Section::Groove);
        for _ in 0..120 {
            color = approach(color, section_color(Section::Peak), 1.0 / 60.0);
        }
        let target = section_color(Section::Peak);
        assert!(color.iter().zip(target).all(|(a, b)| (a - b).abs() < 0.01), "{color:?}");
    }

    #[test]
    fn un_kick_fait_pulser_le_cube_et_la_section_le_colore() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)))
            .init_resource::<CubeMood>()
            .init_resource::<AudioFeatures>()
            .add_message::<Kick>()
            .add_systems(Update, react_to_audio);
        app.update();
        assert!(app.world().resource::<CubeMood>().pulse < 0.01);

        app.world_mut().write_message(Kick {
            audio_time_s: 0.0,
            step: 1,
        });
        app.update();
        assert!(app.world().resource::<CubeMood>().pulse > 0.99);

        app.update();
        app.update();
        let pulse = app.world().resource::<CubeMood>().pulse;
        assert!(pulse < 0.99 && pulse > 0.0, "{pulse}");

        let mut features = *app.world().resource::<AudioFeatures>();
        features.0.section = Section::Peak;
        app.insert_resource(features);
        for _ in 0..90 {
            app.update();
        }
        let color = app.world().resource::<CubeMood>().color;
        assert!(color[0] > 0.9, "{color:?}");
    }
}
