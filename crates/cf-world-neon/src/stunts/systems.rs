//! La mince couche Bevy des cascades : extrait les entrées de l'image, appelle le directeur, écrit la pose fondue
//! (position et animation de la voiture) et publie [`StuntOrders`]. Toute la décision est dans
//! [`StuntDirector`](super::StuntDirector).

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, CurrentShow, ScenePalette, ShowRuntime};
use cf_show::director::Order;

use super::director::{StuntDirector, StuntFrame, StuntView};
use super::kind::StuntKind;
use super::permit::StuntLink;
use super::script::{Planned, StuntRegistry, StuntShow};
use super::world::TrafficStuntWorld;
use crate::pilot::rand::Rand;
use crate::pilot::{CarAnimation, PilotBrain, PilotCar};
use crate::traffic::{TrafficBeat, TrafficDirector};
use crate::world::NeonWorld;

/// Réglages des cascades.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StuntSettings {
    /// Graine du hasard des cascades : mêmes entrées, mêmes cascades.
    pub seed: u64,
    /// Mode « réduire les animations » : aucune cascade.
    pub calm: bool,
}

impl Default for StuntSettings {
    fn default() -> Self {
        Self {
            seed: 0x57_00_7C_A5,
            calm: false,
        }
    }
}

/// Les ordres des cascades à l'image : ce que lisent la voiture (effets), le trafic et le réalisateur. Écrit par le
/// seul système [`drive_stunts`].
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct StuntOrders {
    /// Poids de la pose sur la conduite normale, 0 → 1.
    pub weight: f32,
    /// Ni traînée ni fumée : la voiture vole, roule sur un mur ou hors de la route.
    pub fx_off: bool,
    /// La cascade en cours, pour le réalisateur (plan `stunt-cam`).
    pub view: Option<StuntView>,
}

impl StuntOrders {
    /// Une cascade tient la voiture.
    #[must_use]
    pub fn active(&self) -> bool {
        self.view.is_some()
    }
}

/// Le chef du spectacle vu des cascades : l'ordre « Ascension » et la mémoire d'anti-répétition.
struct RuntimeLink<'a>(&'a mut ShowRuntime);

impl StuntLink for RuntimeLink<'_> {
    fn take_ascension(&mut self) -> bool {
        self.0.director_mut().take(Order::Ascension)
    }

    fn freshness_weight(&self, id: &str, now: f64) -> f64 {
        self.0.director().freshness().weight(id, now)
    }

    fn freshness_mark(&mut self, id: &str, now: f64) {
        self.0.director_mut().freshness_mut().mark(id, now);
    }

    fn request_time_scale(&mut self, scale: f32, seconds: f32) {
        self.0.director_mut().request_time_scale(scale, seconds);
    }
}

/// Le hasard des véhicules posés par les cascades : il ne sert qu'à leur déphasage de brillance.
pub(crate) struct SpawnRng(Rand);

impl Default for SpawnRng {
    fn default() -> Self {
        Self(Rand::new(0x5C47))
    }
}

fn linear(c: cf_render::palette::LinearRgb) -> LinearRgba {
    LinearRgba::rgb(c.r, c.g, c.b)
}

/// `CfSet::Simulate`, après le pilote et avant le pas du trafic : décide, fond la pose sur la conduite du pilote
/// (abscisse, altitude, inclinaisons) et publie les ordres.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(crate) fn drive_stunts(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    runtime: Option<ResMut<ShowRuntime>>,
    palette: Option<Res<ScenePalette>>,
    brain: Option<Res<PilotBrain>>,
    city: Option<Res<NeonWorld>>,
    settings: Res<StuntSettings>,
    registry: Res<StuntRegistry>,
    mut director: ResMut<StuntDirector>,
    mut traffic: ResMut<TrafficDirector>,
    mut animation: ResMut<CarAnimation>,
    mut orders: ResMut<StuntOrders>,
    mut cars: Query<&mut AbsolutePosition, With<PilotCar>>,
    mut spawn_rng: Local<SpawnRng>,
) {
    let Ok(mut position) = cars.single_mut() else { return };
    let features = features.as_deref().map(|f| f.0);
    let planned = runtime
        .as_deref()
        .and_then(|r| r.director().drop_plan())
        .and_then(|plan| plan.stunt.as_ref())
        .map_or(Planned::None, |kind| {
            StuntKind::from_slug(kind.as_str()).map_or(Planned::Unknown, Planned::Kind)
        });
    let show_view = show.as_deref().map(|s| {
        let state = &s.0;
        StuntShow {
            gates: state.gates(),
            breathing: state.breathing,
            drop_now: state.drop_now,
            planned,
        }
    });
    let frame = StuntFrame {
        dt: f64::from(time.delta_secs().min(0.1)),
        now: time.elapsed_secs_f64(),
        // Pilote en marche, source audio vivante (le monde ralentit sans elle), pas de mouvement réduit.
        enabled: features.is_some_and(|f| f.live) && !settings.calm,
        car_x: position.x,
        car_z: position.z,
        altitude: f64::from(animation.altitude),
        road_speed: brain.as_deref().map_or(0.0, |b| b.0.frame().speed),
        beat: features.as_ref().map_or(TrafficBeat::SILENT, TrafficBeat::from),
        // Hors spectacle, aucune source de drop : seul le chef du spectacle en annonce (`drop_now`).
        drop: false,
        intensity: features.map_or(0.4, |f| f64::from(f.intensity)),
        show: show_view,
    };

    let director = &mut *director;
    let mut world = TrafficStuntWorld {
        traffic: &mut traffic,
        city: city.as_deref(),
        cycle: palette
            .as_deref()
            .map_or([LinearRgba::WHITE; 6], |p| p.prismatic_cycle().map(linear)),
        rng: &mut spawn_rng.0,
    };
    let x = match runtime {
        Some(mut runtime) => director.update(&frame, &registry, &mut world, &mut RuntimeLink(&mut runtime)),
        None => director.update(&frame, &registry, &mut world, &mut super::permit::NoLink),
    };
    let grounded = show_view.is_some_and(|s| s.gates.max_floor == 0);
    if director.active() && !grounded {
        let mut blended = *animation;
        director.apply(&mut blended);
        animation.set_if_neq(blended);
        let next = DVec3::new(x, f64::from(blended.altitude), position.z);
        if position.0 != next {
            position.0 = next;
        }
    }
    orders.set_if_neq(StuntOrders {
        weight: director.weight() as f32,
        fx_off: director.fx_off(),
        view: if grounded { None } else { director.view() },
    });
}
