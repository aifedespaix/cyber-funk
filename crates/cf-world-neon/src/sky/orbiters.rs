//! Les systèmes des astres en orbite ([`ORBITERS`]) : naissance au démarrage, mouvement à chaque image.

use bevy::prelude::*;

use super::SkyOrbiter;
use super::material::{PlanetMaterial, PlanetParams};
use super::orbit::ORBITERS;
use super::systems::SkyState;
use super::textures;

/// Crée les astres en orbite : même sphère et même matériau que les autres, mais la position vient de leur trajectoire.
/// Un astre dont la texture ne se décode pas est omis, avec un avertissement.
pub(super) fn spawn_orbiters(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    materials: &mut Assets<PlanetMaterial>,
    sphere: &Handle<Mesh>,
) -> Vec<(usize, Handle<PlanetMaterial>)> {
    let mut orbiters = Vec::new();
    for (index, spec) in ORBITERS.iter().enumerate() {
        let image = match textures::load(spec.body.texture) {
            Ok(image) => image,
            Err(error) => {
                tracing::warn!(%error, planet = spec.body.texture, "astre en orbite omis du ciel");
                continue;
            }
        };
        let material = materials.add(PlanetMaterial {
            params: PlanetParams::default(),
            map: Some(images.add(image)),
        });
        commands.spawn((
            Name::new(format!("ciel · orbite {}", spec.body.texture)),
            SkyOrbiter(index),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: spec.orbit.position(0.0),
                rotation: spec.body.rotation(0.0),
                scale: Vec3::splat(spec.body.radius_m),
            },
        ));
        orbiters.push((index, material));
    }
    orbiters
}

/// `CfSet::Simulate` : fait avancer la Lune et la géante sur leur orbite et tourner sur leur axe.
///
/// Une horloge à part ([`SkyState::elapsed_s`]) : la position est une fonction du temps, jamais une somme de petits pas qui
/// dériverait.
pub(super) fn step_orbiters(
    time: Res<Time>,
    mut state: ResMut<SkyState>,
    mut orbiters: Query<(&SkyOrbiter, &mut Transform)>,
) {
    let dt = time.delta_secs();
    state.elapsed_s += f64::from(dt);
    let elapsed = state.elapsed_s;
    for (orbiter, mut transform) in &mut orbiters {
        let spec = &ORBITERS[orbiter.0];
        let spin = &mut state.orbiter_spin_rad[orbiter.0];
        *spin = (*spin + spec.spin_step_rad(dt)).rem_euclid(std::f32::consts::TAU);
        transform.translation = spec.orbit.position(elapsed);
        transform.rotation = spec.body.rotation(*spin);
    }
}
