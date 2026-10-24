//! Provisoire (J5-01) : un point suivi qui avance tout droit, et la caméra de poursuite de Sillage derrière lui.
//!
//! Sans voiture ni réalisateur, rien ne fait avancer le monde ni ne le filme. Ce module remplit les deux rôles le temps
//! que les vrais arrivent : la voiture en pilote automatique (J5-05, J5-06) remplacera le [`Runner`], le réalisateur
//! (J6) remplacera la caméra. Désactivé par [`crate::NeonWorldPlugin::embedded`] quand l'un des deux est là : deux
//! écrivains de `CameraPose` se disputeraient l'image.

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, CameraPose, CfSet, StreamFocus, WorldLayer, WorldSet};

/// Vitesse de croisière du point suivi, en m/s (de l'ordre de la vitesse de Sillage en groove).
pub const CRUISE_SPEED_MPS: f64 = 40.0;

/// Le point suivi provisoire : un pavé lumineux qui avance vers −z. Porte [`StreamFocus`] : la fenêtre de tronçons, la
/// route et l'origine flottante le suivent.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(StreamFocus, WorldLayer)]
pub struct Runner;

/// Réglages du point suivi provisoire.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RunnerConfig {
    /// Profondeur de départ (z absolu, m) : 0 au départ du set, −400 000 pour vérifier la précision en fin de set.
    pub start_z_m: f64,
    /// Vitesse, m/s.
    pub speed_mps: f64,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            start_z_m: 0.0,
            speed_mps: CRUISE_SPEED_MPS,
        }
    }
}

/// Pose de la caméra de poursuite de Sillage (`cameras.ts`, mode `chase`, voiture au sol, sans élan) : un peu derrière et
/// au-dessus de la voiture, qui regarde un point 8 m devant elle. Le lissage de Sillage (suivi relatif) n'est pas
/// repris : le point suivi avance à vitesse constante, la pose n'a rien à lisser.
///
/// - `pulse` : pouls du kick, 0 à 1 (la caméra monte de 10 cm, le champ s'ouvre de 3°) ;
/// - `intensity` : intensité du morceau, 0 à 1 (le champ s'ouvre jusqu'à 6° de plus).
#[must_use]
pub fn chase_pose(car: DVec3, pulse: f32, intensity: f32) -> CameraPose {
    // La caméra est en (0,55 x, 2,3 + pouls, z + 6,2) et regarde (0,35 x, 1, z − 8), x et z étant ceux de la voiture. Le
    // regard se calcule relativement à la caméra, en `f32` : exact quel que soit l'éloignement ; seule la position
    // finale est absolue (`f64`). En vol, la caméra monte avec la voiture (son altitude `car.y`) : le regard suit, il
    // est relatif.
    let height = 2.3 + pulse * 0.1;
    let x = car.x as f32;
    let to_look = Vec3::new(x * (0.35 - 0.55), 1.0 - height, -8.0 - 6.2);
    let rotation = Transform::IDENTITY.looking_to(to_look, Vec3::Y).rotation;
    CameraPose {
        position: DVec3::new(car.x * 0.55, car.y + f64::from(height), car.z + 6.2),
        rotation,
        fov_deg: 75.0 + intensity * 6.0 + pulse * 3.0,
        ..CameraPose::default()
    }
}

/// Installe le point suivi provisoire et la caméra de poursuite.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChasePlugin {
    /// Réglages de départ.
    pub config: RunnerConfig,
}

impl Plugin for ChasePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config)
            .add_plugins(ChaseCameraPlugin)
            .add_systems(Startup, spawn_runner)
            .add_systems(Update, advance_runner.in_set(CfSet::Simulate).before(WorldSet::Gates));
    }
}

/// Installe la seule caméra de poursuite : elle suit l'entité qui porte [`StreamFocus`], point suivi provisoire ou
/// voiture du pilote ([`crate::PilotPlugin`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct ChaseCameraPlugin;

impl Plugin for ChaseCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraPose>()
            .add_systems(Update, chase_camera.in_set(CfSet::Direct));
    }
}

fn spawn_runner(
    mut commands: Commands,
    config: Res<RunnerConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("point suivi provisoire"),
        Runner,
        AbsolutePosition(DVec3::new(0.0, 0.65, config.start_z_m)),
        Mesh3d(meshes.add(Cuboid::new(1.9, 1.3, 4.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            // Sans éclairage (`unlit`) Bevy ne lit que la couleur de base : le pavé est lumineux, comme son violet de
            // Sillage, mais ne dépend d'aucune lumière.
            base_color: Color::linear_rgb(0.4, 0.15, 1.2),
            unlit: true,
            ..default()
        })),
    ));
}

/// Fait avancer le point suivi vers −z, à vitesse constante.
fn advance_runner(time: Res<Time>, config: Res<RunnerConfig>, mut runner: Query<&mut AbsolutePosition, With<Runner>>) {
    let step = config.speed_mps * time.delta_secs_f64();
    for mut position in &mut runner {
        position.0.z -= step;
    }
}

/// `CfSet::Direct` : la caméra suit l'entité suivie (point suivi provisoire ou voiture).
fn chase_camera(
    features: Option<Res<AudioFeatures>>,
    runner: Query<&AbsolutePosition, With<StreamFocus>>,
    mut pose: ResMut<CameraPose>,
) {
    let Ok(car) = runner.single() else { return };
    let (pulse, intensity) = features.as_deref().map_or((0.0, 0.4), |f| (f.0.pulse, f.0.intensity));
    let next = chase_pose(car.0, pulse, intensity);
    if *pose != next {
        *pose = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_camera_est_derriere_et_au_dessus_et_regarde_devant() {
        let car = DVec3::new(0.0, 0.0, -123_456.0);
        let pose = chase_pose(car, 0.0, 0.4);
        assert!((pose.position.z - (car.z + 6.2)).abs() < 1e-9, "6,2 m derrière");
        assert!((pose.position.y - 2.3).abs() < 1e-6);
        // Regarde vers −z, légèrement vers le bas (de 2,3 m à 1 m, sur 14,2 m).
        let forward = pose.rotation * Vec3::NEG_Z;
        assert!(forward.z < -0.99 && forward.y < 0.0, "{forward}");
        assert!((forward.y - (-1.3_f32 / 14.2).atan().sin()).abs() < 1e-3, "{forward}");
        assert!((pose.fov_deg - 77.4).abs() < 1e-4);
    }

    #[test]
    fn le_pouls_leve_la_camera_et_ouvre_le_champ() {
        let calm = chase_pose(DVec3::ZERO, 0.0, 0.4);
        let kick = chase_pose(DVec3::ZERO, 1.0, 0.4);
        assert!((kick.position.y - calm.position.y - 0.1).abs() < 1e-6);
        assert!((kick.fov_deg - calm.fov_deg - 3.0).abs() < 1e-4);
    }

    #[test]
    fn la_position_reste_absolue_a_400_km() {
        let car = DVec3::new(0.0, 0.0, -400_000.123);
        assert!((chase_pose(car, 0.0, 0.0).position.z - (-400_000.123 + 6.2)).abs() < 1e-9);
    }
}
