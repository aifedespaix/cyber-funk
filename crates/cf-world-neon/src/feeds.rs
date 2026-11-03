//! Les caméras annexes de la voiture : le rétroviseur (vue vers l'arrière) et les flux des enseignes « caméra » (la
//! voiture vue d'un portique, de face, de profil).
//!
//! # Principe
//!
//! Chaque flux est une vraie caméra de Bevy qui rend le monde dans une petite image ; cette image sert ensuite de texture
//! (émissif de `screen_mirror`, fenêtre vidéo du shader des enseignes). Le coût est borné de quatre façons :
//!
//! - **Taille** : 192 × 64 pour le rétroviseur, 256 × 104 pour un plan d'enseigne.
//! - **Cadence** : une caméra n'est active que l'image où elle doit rendre (20 Hz pour le rétroviseur, 24 Hz partagés
//!   entre les plans d'enseigne demandés). Entre deux rendus, l'image garde son dernier contenu.
//! - **Demande** : le rétroviseur ne rend que si l'œil du réalisateur est à moins de [`MIRROR_RANGE_M`] de la voiture ;
//!   un plan d'enseigne ne rend que si une enseigne qui l'affiche est visible ([`FeedDemand`], écrite par `render_signs`).
//! - **Pipelines** : mêmes réglages de vue que la couche monde (HDR, MSAA ×4, sans tonemapping) et même format de cible,
//!   donc aucun pipeline neuf à compiler pendant un show (règle 8).
//!
//! Les caméras sont créées une fois au démarrage et ne sont jamais détruites (règle 4). Elles ne voient que le ciel et
//! le monde : ni avant-plan ni surcouche, ni post-traitement.
//!
//! # Sécurité photosensible (règle 7)
//!
//! Un flux ne montre que ce que le monde montre déjà, sous les limiteurs du monde ; la cadence réduite ne l'accélère
//! pas. Aucun flash n'est ajouté ici.

use bevy::asset::RenderAssetUsages;
use bevy::camera::{Camera, Camera3d, ClearColorConfig, Hdr, PerspectiveProjection, Projection, RenderTarget};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::Msaa;
use cf_render::{AbsolutePosition, CameraPose, CfSet, FloatingOrigin, Layer};

use crate::car::{Car, CarRig};

/// Nombre de plans d'enseigne : large, de face, de profil (l'ordre de `CamShot`).
pub const SHOTS: usize = 3;
/// Taille de l'image du rétroviseur, px.
pub const REAR_SIZE: UVec2 = UVec2::new(192, 64);
/// Taille de l'image d'un plan d'enseigne, px (le rapport de la fenêtre vidéo du shader, 492 × 202).
pub const SHOT_SIZE: UVec2 = UVec2::new(256, 104);
/// Distance (m) entre l'œil du réalisateur et la voiture en deçà de laquelle le rétroviseur se redessine.
pub const MIRROR_RANGE_M: f64 = 12.0;
/// Cadence du rétroviseur, Hz.
const REAR_HZ: f32 = 20.0;
/// Cadence totale des plans d'enseigne, Hz (partagée entre les plans demandés).
const SHOT_HZ: f32 = 24.0;
/// Format des cibles : celui des couches de `cf-render`, pour réutiliser leurs pipelines.
const FEED_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// Quel flux filme une caméra annexe.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedCamera {
    /// Le rétroviseur : derrière la voiture, vers l'arrière.
    Rear,
    /// Un plan d'enseigne (0 large, 1 de face, 2 de profil).
    Shot(u8),
}

impl FeedCamera {
    /// Champ vertical, degrés.
    const fn fov_deg(self) -> f32 {
        match self {
            Self::Rear => 38.0,
            Self::Shot(1) => 55.0,
            Self::Shot(_) => 45.0,
        }
    }
}

/// Les images où les caméras annexes rendent.
#[derive(Resource, Debug, Clone)]
pub struct FeedImages {
    /// Le rétroviseur.
    pub rear: Handle<Image>,
    /// Les plans d'enseigne.
    pub shots: [Handle<Image>; SHOTS],
}

fn target(size: UVec2) -> Image {
    let mut image = Image::new_target_texture(size.x, size.y, FEED_FORMAT, None);
    // Effacée par sa caméra à chaque rendu : aucune copie de zéros côté processeur, aucune ancienne texture à recopier.
    image.data = None;
    image.copy_on_resize = false;
    image.sampler = ImageSampler::linear();
    image.asset_usage = RenderAssetUsages::default();
    image
}

impl FromWorld for FeedImages {
    fn from_world(world: &mut World) -> Self {
        // Sans rendu (tests de logique), des poignées vides : aucune caméra ne sera créée.
        let Some(mut images) = world.get_resource_mut::<Assets<Image>>() else {
            return Self {
                rear: Handle::default(),
                shots: [(); SHOTS].map(|()| Handle::default()),
            };
        };
        Self {
            rear: images.add(target(REAR_SIZE)),
            shots: [(); SHOTS].map(|()| images.add(target(SHOT_SIZE))),
        }
    }
}

/// Les plans que des enseignes visibles affichent à cette image. Écrite par `render_signs`, lue par [`drive_feeds`].
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedDemand {
    /// Un plan par entrée.
    pub shots: [bool; SHOTS],
}

/// Les flux qui ont déjà rendu au moins une fois : avant cela l'image est vide, et l'enseigne garde son dessin de repli.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedReady {
    /// Le rétroviseur.
    pub rear: bool,
    /// Les plans d'enseigne.
    pub shots: [bool; SHOTS],
}

/// Quelles caméras rendent à cette image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tick {
    /// Le rétroviseur rend.
    pub rear: bool,
    /// Le plan d'enseigne qui rend.
    pub shot: Option<usize>,
}

/// L'horloge de cadence des flux : logique pure.
#[derive(Debug, Clone, Copy, Default)]
pub struct FeedClock {
    rear_s: f32,
    shot_s: f32,
    next: usize,
}

impl FeedClock {
    /// Avance d'une image. Une horloge qui n'a rien à faire reste « échue » : le premier besoin rend sans attendre.
    pub fn step(&mut self, dt_s: f32, demand: [bool; SHOTS], rear_wanted: bool) -> Tick {
        let (rear_period, shot_period) = (1.0 / REAR_HZ, 1.0 / SHOT_HZ);
        let mut tick = Tick::default();
        if rear_wanted {
            self.rear_s += dt_s;
            if self.rear_s >= rear_period {
                self.rear_s = 0.0;
                tick.rear = true;
            }
        } else {
            self.rear_s = rear_period;
        }
        self.shot_s = (self.shot_s + dt_s).min(shot_period);
        if self.shot_s >= shot_period {
            // Tourniquet : le plan demandé qui suit le dernier rendu.
            if let Some(shot) = (0..SHOTS).map(|i| (self.next + i) % SHOTS).find(|&s| demand[s]) {
                self.next = (shot + 1) % SHOTS;
                self.shot_s = 0.0;
                tick.shot = Some(shot);
            }
        }
        tick
    }
}

/// Place d'une caméra dans le repère de la voiture (l'avant est −Z), `half_length` étant la demi-longueur de la caisse.
#[must_use]
pub fn local_pose(kind: FeedCamera, half_length: f32) -> Transform {
    let car = Vec3::new(0.0, 0.7, 0.0);
    match kind {
        // Au pare-chocs arrière, regard vers l'arrière.
        FeedCamera::Rear => Transform::from_xyz(0.0, 1.05, half_length + 0.1).looking_to(Vec3::Z, Vec3::Y),
        // Du portique : devant la voiture, en hauteur.
        FeedCamera::Shot(0) => Transform::from_xyz(0.0, 5.5, -17.0).looking_at(car, Vec3::Y),
        // De face, devant le capot.
        FeedCamera::Shot(1) => Transform::from_xyz(0.0, 0.95, -5.8).looking_at(car, Vec3::Y),
        // De profil.
        FeedCamera::Shot(_) => Transform::from_xyz(7.5, 1.2, -0.5).looking_at(car, Vec3::Y),
    }
}

/// Installe les images, les caméras annexes et leur pilotage. Ajouté par `CarPlugin` et `SignsPlugin`.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeedsPlugin;

impl Plugin for FeedsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FeedImages>()
            .init_resource::<FeedDemand>()
            .init_resource::<FeedReady>();
        // Sans rendu (tests de logique pure), pas de caméra.
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_systems(Startup, spawn_cameras).add_systems(
                Update,
                drive_feeds
                    .in_set(CfSet::Present)
                    .after(crate::signs::systems::render_signs),
            );
        }
    }
}

/// `Startup` : une caméra par flux, éteinte. Elle voit le ciel et le monde, comme les couches 0 et 1.
fn spawn_cameras(mut commands: Commands, images: Res<FeedImages>) {
    let layers = Layer::World.render_layers().with(Layer::Background.index());
    let cameras = std::iter::once((FeedCamera::Rear, images.rear.clone(), -5_isize)).chain(
        images
            .shots
            .iter()
            .enumerate()
            .map(|(i, image)| (FeedCamera::Shot(i as u8), image.clone(), -4 + i.cast_signed())),
    );
    for (kind, image, order) in cameras {
        commands.spawn((
            Name::new(format!("flux {kind:?}")),
            kind,
            Camera3d::default(),
            Camera {
                order,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                // Allumée seulement l'image où elle rend (`drive_feeds`).
                is_active: false,
                ..default()
            },
            RenderTarget::from(image),
            layers.clone(),
            Hdr,
            Projection::Perspective(PerspectiveProjection {
                fov: kind.fov_deg().to_radians(),
                near: 0.1,
                far: 1_500.0,
                ..default()
            }),
            // Les réglages des couches 3D de `cf-render` : mêmes pipelines.
            Tonemapping::None,
            DebandDither::Disabled,
            Msaa::Sample4,
            Transform::default(),
        ));
    }
}

/// Ce que les caméras lisent de la voiture : sa position absolue, son orientation et sa caisse.
type CarFeedQuery = (&'static AbsolutePosition, &'static Transform, &'static CarRig);

/// `CfSet::Present`, après les enseignes : place les caméras sur la voiture et n'allume que celles qui rendent.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
fn drive_feeds(
    time: Res<Time>,
    demand: Res<FeedDemand>,
    pose: Option<Res<CameraPose>>,
    origin: Option<Res<FloatingOrigin>>,
    cars: Query<CarFeedQuery, (With<Car>, Without<FeedCamera>)>,
    mut ready: ResMut<FeedReady>,
    mut clock: Local<FeedClock>,
    mut cameras: Query<(&FeedCamera, &mut Camera, &mut Transform), Without<Car>>,
) {
    let car = cars.single().ok();
    let near = car
        .zip(pose)
        .is_some_and(|((position, ..), pose)| (pose.position - position.0).length() < MIRROR_RANGE_M);
    let tick = if car.is_some() {
        clock.step(time.delta_secs(), demand.shots, near)
    } else {
        Tick::default()
    };
    let origin = origin.map_or(DVec3::ZERO, |origin| origin.origin());
    for (&kind, mut camera, mut transform) in &mut cameras {
        let due = match kind {
            FeedCamera::Rear => tick.rear,
            FeedCamera::Shot(i) => tick.shot == Some(usize::from(i)),
        };
        if due && let Some((position, root, rig)) = car {
            let root = Transform {
                translation: (position.0 - origin).as_vec3(),
                rotation: root.rotation,
                scale: Vec3::ONE,
            };
            transform.set_if_neq(root * local_pose(kind, rig.frame.half_length()));
        }
        // Rendue l'image précédente, l'image est maintenant prête.
        if camera.is_active && !due {
            match kind {
                FeedCamera::Rear => ready.rear = true,
                FeedCamera::Shot(i) => ready.shots[usize::from(i).min(SHOTS - 1)] = true,
            }
        }
        if camera.is_active != due {
            camera.is_active = due;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(clock: &mut FeedClock, seconds: f32, demand: [bool; SHOTS], rear: bool) -> (u32, [u32; SHOTS]) {
        let (mut rears, mut shots) = (0, [0; SHOTS]);
        for _ in 0..(seconds / DT) as u32 {
            let tick = clock.step(DT, demand, rear);
            rears += u32::from(tick.rear);
            if let Some(shot) = tick.shot {
                shots[shot] += 1;
            }
        }
        (rears, shots)
    }

    #[test]
    fn sans_demande_rien_ne_rend() {
        let (rears, shots) = run(&mut FeedClock::default(), 2.0, [false; SHOTS], false);
        assert_eq!(rears, 0);
        assert_eq!(shots, [0; SHOTS]);
    }

    #[test]
    fn le_retroviseur_rend_a_sa_cadence() {
        let (rears, _) = run(&mut FeedClock::default(), 2.0, [false; SHOTS], true);
        assert!((38..=41).contains(&rears), "{rears} rendus en 2 s");
    }

    #[test]
    fn les_plans_demandes_se_partagent_la_cadence() {
        let (_, shots) = run(&mut FeedClock::default(), 3.0, [true, false, true], false);
        assert_eq!(shots[1], 0, "un plan non demandé ne rend jamais");
        assert!(shots[0].abs_diff(shots[2]) <= 1, "{shots:?}");
        let total = shots.iter().sum::<u32>();
        // 24 Hz arrondi à l'image : une image sur trois à 60 i/s, soit ~20 Hz.
        assert!((55..=74).contains(&total), "{total} rendus en 3 s");
    }

    #[test]
    fn le_premier_besoin_rend_sans_attendre() {
        let mut clock = FeedClock::default();
        run(&mut clock, 1.0, [false; SHOTS], false);
        assert_eq!(clock.step(DT, [false, true, false], false).shot, Some(1));
        let mut clock = FeedClock::default();
        run(&mut clock, 1.0, [false; SHOTS], false);
        assert!(clock.step(DT, [false; SHOTS], true).rear);
    }

    #[test]
    fn le_retroviseur_regarde_vers_l_arriere() {
        let pose = local_pose(FeedCamera::Rear, 2.3);
        assert!(pose.forward().dot(Vec3::Z) > 0.999, "{:?}", pose.forward());
        assert!(pose.translation.z > 2.3, "hors de la caisse");
    }

    #[test]
    fn les_plans_d_enseigne_cadrent_la_voiture() {
        for i in 0..SHOTS as u8 {
            let pose = local_pose(FeedCamera::Shot(i), 2.3);
            let to_car = (Vec3::new(0.0, 0.7, 0.0) - pose.translation).normalize();
            assert!(pose.forward().dot(to_car) > 0.999, "plan {i}");
        }
    }

    #[test]
    fn les_cibles_sont_dans_le_format_des_couches() {
        let image = target(SHOT_SIZE);
        assert_eq!(image.texture_descriptor.format, FEED_FORMAT);
        assert_eq!(image.texture_descriptor.size.width, SHOT_SIZE.x);
    }
}
