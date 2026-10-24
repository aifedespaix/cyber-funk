//! Le branchement de Neon City sur le réalisateur (J6-01) : la ville répond aux questions de collision
//! ([`Obstacles`]) et recompose à chaque image le [`CinemaWorld`] (voiture du pilote, audio, spectacle, ciel), comme
//! `CinemaWorldSource` de `cinema/world.ts`.
//!
//! [`NeonCinemaPlugin`] remplace la caméra de poursuite provisoire ([`crate::ChaseCameraPlugin`]) : un seul écrivain de
//! `CameraPose`. À utiliser avec [`crate::NeonWorldPlugin::embedded`], le pilote et `cf_cinema::CinemaPlugin`.

use std::f32::consts::TAU;

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_analysis::AudioFeatures as Features;
use cf_cinema::{
    BeatView, Cabin, CinemaWorld, CinemaWorldPlugin, FlightPhase as CinemaPhase, MAX_DT, Obstacles,
    RingPlanet as CinemaRing, ShowView, Trick, TrickKind as CinemaTrick,
};
use cf_render::{AbsolutePosition, AudioFeatures, CfSet, CurrentShow, StreamFocus, WorldSet};
use cf_show::director::ShowState;

use crate::pilot::{CarAnimation, DodgeSide, FlightPhase, PilotBrain, PilotSet, TrickKind};
use crate::sky::RingPlanet;
use crate::world::NeonWorld;

/// Les yeux du conducteur, relatifs à la voiture (m) : au-dessus du siège, un peu en arrière du centre.
const CABIN_OFFSET: Vec3 = Vec3::new(0.0, 1.05, 0.3);

impl Obstacles for NeonWorld {
    fn collides(&self, at: DVec3, radius: f64) -> bool {
        NeonWorld::collides(self, at, radius)
    }
}

/// Ce que la scène sait de l'image en cours (`WorldFrame` de Sillage).
#[derive(Debug, Clone, Copy)]
pub struct CinemaInput<'a> {
    /// Horloge de la scène (s).
    pub time: f64,
    /// Pas de l'image (s).
    pub dt: f64,
    /// Position de la voiture, altitude comprise (absolue, m).
    pub car: DVec3,
    /// Vitesse le long de la route (m/s).
    pub speed: f64,
    /// L'animation de vol publiée par le pilote.
    pub animation: &'a CarAnimation,
    /// L'audio de l'image.
    pub features: &'a Features,
    /// Le spectacle de l'image, s'il y en a un.
    pub show: Option<&'a ShowState>,
    /// La géante à anneau du ciel, si elle est affichée.
    pub ring: Option<&'a RingPlanet>,
}

/// Recompose `world` pour cette image. Aucune allocation : tout est copié en place.
pub fn fill_cinema_world(world: &mut CinemaWorld, input: &CinemaInput<'_>) {
    let dt = (input.dt as f32).clamp(0.0, MAX_DT);
    let previous = world.car;
    let had_previous = world.time > 0.0;
    let section = input.features.section;
    world.section_changed = (had_previous && section != world.section).then_some(section);
    world.time = input.time;
    world.dt = dt;
    world.car = input.car;
    world.car_velocity = if had_previous && input.dt > 0.0 {
        ((input.car - previous) / input.dt).as_vec3()
    } else {
        Vec3::new(0.0, 0.0, -(input.speed as f32))
    };
    let a = input.animation;
    let v = world.car_velocity;
    // Lacet de la voiture d'après sa trajectoire (la route file vers −z), puis l'assiette de vol.
    let yaw = if v.x.abs() + v.z.abs() > 0.1 {
        (-v.x).atan2(-v.z)
    } else {
        0.0
    };
    world.car_rotation = Quat::from_rotation_y(yaw) * a.orientation();
    world.cabin = Cabin {
        position: input.car + (world.car_rotation * CABIN_OFFSET).as_dvec3(),
        forward: world.car_rotation * Vec3::NEG_Z,
        up: world.car_rotation * Vec3::Y,
    };
    world.speed = input.speed as f32;
    world.altitude = a.altitude;
    world.animation = cf_cinema::CarAnimation {
        phase: match a.phase {
            FlightPhase::Ground => CinemaPhase::Ground,
            FlightPhase::Deploying => CinemaPhase::Deploying,
            FlightPhase::Flying => CinemaPhase::Flying,
            FlightPhase::Retracting => CinemaPhase::Retracting,
        },
        hover: a.hover_amount,
        climb: a.climb,
        pitch: a.pitch,
        roll: a.roll,
        bounce: a.bounce,
        dodge: match a.dodge {
            DodgeSide::None => 0,
            DodgeSide::Under => -1,
            DodgeSide::Over => 1,
        },
    };
    world.trick = a.trick.map(|kind| Trick {
        kind: match kind {
            TrickKind::RollLeft | TrickKind::RollRight => CinemaTrick::Roll,
            TrickKind::Backflip | TrickKind::Frontflip => CinemaTrick::Flip,
        },
        progress: (a.trick_roll.abs().max(a.trick_pitch.abs()) / TAU).clamp(0.0, 1.0),
    });
    let f = input.features;
    world.beat = BeatView {
        count: f.count,
        bpm: f.bpm,
        pulse: f.pulse,
    };
    world.section = section;
    world.intensity = f.intensity;
    world.live = f.live;
    world.show = input.show.map_or_else(ShowView::default, ShowView::of);
    // Le drop de Sillage était le boost du jeu ; ici c'est la frappe du chef du spectacle.
    world.drop = world.show.drop_now;
    world.ring_planet = input.ring.map(|r| CinemaRing {
        center: r.center,
        radius: r.radius,
        ring_inner: r.ring_inner,
        ring_outer: r.ring_outer,
        ring_normal: r.ring_normal,
    });
    // Ni cascades ni astres qui tombent dans Neon City pour l'instant.
    world.stunt = None;
    world.last_blast = None;
}

/// Branche Neon City sur le réalisateur.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeonCinemaPlugin;

impl Plugin for NeonCinemaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CinemaWorldPlugin::<NeonWorld>::default()).add_systems(
            Update,
            build_cinema_world
                .in_set(CfSet::Simulate)
                .after(WorldSet::Chunks)
                .after(PilotSet),
        );
    }
}

/// `CfSet::Simulate`, après le pilote et les tronçons : le monde du réalisateur pour cette image.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
fn build_cinema_world(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    brain: Option<Res<PilotBrain>>,
    animation: Option<Res<CarAnimation>>,
    ring: Option<Res<RingPlanet>>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut world: ResMut<CinemaWorld>,
) {
    let Ok(car) = car.single() else { return };
    let neutral = CarAnimation::default();
    let speed = brain.as_deref().map_or(0.0, |b| b.0.frame().speed);
    let features = features.as_deref().map_or(Features::NEUTRAL, |f| f.0);
    fill_cinema_world(
        &mut world,
        &CinemaInput {
            time: time.elapsed_secs_f64(),
            dt: time.delta_secs_f64(),
            car: car.0,
            speed,
            animation: animation.as_deref().unwrap_or(&neutral),
            features: &features,
            show: show.as_deref().map(|s| &s.0),
            ring: ring.as_deref(),
        },
    );
}

#[cfg(test)]
mod tests;
