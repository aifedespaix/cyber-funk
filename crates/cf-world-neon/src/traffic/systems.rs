//! La mince couche Bevy du trafic : extrait les entrées de l'image, appelle le directeur, publie ce qu'il produit (ce que
//! le pilote doit voir, les zones à dégager de la ville, les compteurs). Toute la décision est dans
//! [`TrafficDirector`](super::TrafficDirector).

use bevy::color::LinearRgba;
use bevy::ecs::system::SystemParam;
use bevy::math::{DVec3, Vec4};
use bevy::prelude::*;
use cf_render::{
    AbsolutePosition, AudioFeatures, ChunkStream, CurrentShow, CurrentShowFx, FloatingOrigin, QualityLevers,
    SceneColor, ScenePalette, ShowRuntime, SimulationLod, StreamFocus,
};
use cf_show::director::Order;

use super::director::TrafficDirector;
use super::event::{EventRegistry, SignLender};
use super::flash::TrafficFlash;
use super::fleet::FleetStats;
use super::frame::{TrafficBeat, TrafficDrive, TrafficFrame, TrafficShow};
use super::names::EventName;
use super::render::TrafficEntities;
use super::scheduler::{NoShow, ShowLink};
use crate::facade::{FacadeLook, NO_BLACKOUT};
use crate::pilot::{CarAnimation, PilotTraffic};
use crate::route::floor_altitude;
use crate::signs::{SignRoadRes, SignShow};
use crate::world::NeonWorld;

/// Écart maximal (s) qu'une image peut faire avancer le trafic : après une pause ou un blocage, on n'avance pas de
/// plusieurs secondes d'un coup (les véhicules traverseraient la fenêtre en une image).
const MAX_DT_S: f32 = 0.1;

/// Réglages du trafic.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrafficSettings {
    /// Graine du hasard du trafic : mêmes entrées, même ville vivante.
    pub seed: u64,
    /// Mode « réduire les animations » : lent et grand seulement.
    pub calm: bool,
    /// Il y a un nom de DJ ou d'événement à afficher : le titre holographique peut être tiré (J7-04).
    pub titles: bool,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        Self {
            seed: 0x7AFF_1C00,
            calm: false,
            titles: false,
        }
    }
}

/// Les compteurs de la dernière image : de quoi vérifier la cadence réduite et la densité (critères de J7-03).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrafficStats {
    /// Les véhicules de la flotte : vivants, pas faits, pas sautés.
    pub fleet: FleetStats,
    /// Véhicules au sol.
    pub ground: usize,
    /// Événements en cours.
    pub events: usize,
    /// Points lumineux posés.
    pub glints: usize,
    /// Murs de lumière actifs.
    pub walls: usize,
    /// Entités préallouées par le trafic (constantes en régime établi).
    pub entities: usize,
}

/// État que le système garde d'une image à l'autre.
#[derive(Debug, Default)]
pub(crate) struct DriveState {
    last_z: Option<f64>,
    speed: f32,
    level: Option<u8>,
    level_seq: u32,
}

/// Le chef du spectacle vu par le trafic : l'ordre « événement au hasard » et la mémoire d'anti-répétition.
struct RuntimeLink<'a>(&'a mut ShowRuntime);

impl ShowLink for RuntimeLink<'_> {
    fn take_random_event(&mut self) -> bool {
        self.0.director_mut().take(Order::RandomEvent)
    }

    fn freshness_weight(&self, id: &str, now: f64) -> f64 {
        self.0.director().freshness().weight(id, now)
    }

    fn freshness_mark(&mut self, id: &str, now: f64) {
        self.0.director_mut().freshness_mut().mark(id, now);
    }
}

fn linear(c: cf_render::palette::LinearRgb) -> LinearRgba {
    LinearRgba::rgb(c.r, c.g, c.b)
}

/// `CfSet::Simulate`, avant le pilote : publie dans [`PilotTraffic`] ce que le trafic a décidé à l'image précédente (les
/// voitures au sol à dépasser, les tranches d'altitude occupées, les structures à esquiver, les dépassements réussis).
pub(crate) fn publish_traffic(
    mut director: ResMut<TrafficDirector>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut traffic: ResMut<PilotTraffic>,
) {
    let Ok(car) = car.single() else { return };
    traffic.ground.clear();
    traffic.bands.clear();
    traffic.obstructions.clear();
    director.ground.views(&director.fleet, &mut traffic.ground, car.z);
    director.airspace(&mut traffic.bands, car.z);
    traffic.obstructions.extend_from_slice(director.obstructions());
    while director.ground.take_overtake() {
        traffic.overtakes += 1;
    }
}

/// Ce que le trafic emprunte aux enseignes : le nom du DJ pour le titre holographique, le gestionnaire de panneaux prêté
/// aux événements.
#[derive(SystemParam)]
pub(crate) struct SignAccess<'w> {
    names: Option<Res<'w, SignShow>>,
    road: Option<ResMut<'w, SignRoadRes>>,
}

/// `CfSet::Simulate`, avant `drive_traffic` : prend le plus ancien déclencheur du monde (`Action::Trigger`) et lance
/// l'événement du trafic qu'il nomme, quelles que soient les portes du look (c'est l'opérateur qui le veut).
///
/// Un identifiant qui n'est pas un événement du trafic (une cascade, par exemple) n'est pas pris : il reste dans la file
/// pour le système à qui il est destiné. Un seul déclencheur par image : `TrafficDirector::force` ne retient qu'un nom.
pub(crate) fn take_triggers(runtime: Option<ResMut<ShowRuntime>>, mut director: ResMut<TrafficDirector>) {
    let Some(mut runtime) = runtime else { return };
    let wanted = runtime
        .director()
        .state()
        .orders
        .trigger_pending()
        .and_then(|id| EventName::from_slug(&id.0));
    if let Some(name) = wanted
        && runtime.director_mut().take_trigger().is_some()
    {
        director.force(name);
    }
}

/// Le nom à afficher en titre : celui du DJ en cours, à défaut celui de l'événement.
fn dj_title(names: &SignShow) -> Option<&str> {
    let slot = names.active.then(|| names.current()).flatten()?;
    [slot.name.as_str(), slot.event.as_str()]
        .into_iter()
        .find(|text| !text.is_empty())
}

/// `CfSet::Simulate`, après la cadence de simulation et le pilote : avance le trafic d'une image.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(crate) fn drive_traffic(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    mut runtime: Option<ResMut<ShowRuntime>>,
    levers: Option<Res<QualityLevers>>,
    palette: Option<Res<ScenePalette>>,
    settings: Res<TrafficSettings>,
    animation: Res<CarAnimation>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    lods: Query<&SimulationLod>,
    entities: Option<Res<TrafficEntities>>,
    mut director: ResMut<TrafficDirector>,
    mut registry: ResMut<EventRegistry>,
    mut stats: ResMut<TrafficStats>,
    mut signs: SignAccess,
    mut state: Local<DriveState>,
) {
    let Ok(car) = car.single() else { return };
    let dt = time.delta_secs().min(MAX_DT_S);
    let now = time.elapsed_secs_f64();

    // Vitesse de la voiture, mesurée sur sa position (m/s, positive vers −z), lissée.
    if let Some(last) = state.last_z
        && dt > 0.0
    {
        let v = ((last - car.z) / f64::from(dt)) as f32;
        if v.is_finite() && (0.0..400.0).contains(&v) {
            state.speed += (v - state.speed) * (1.0 - (-dt * 6.0).exp());
        }
    }
    state.last_z = Some(car.z);

    let beat = features
        .as_deref()
        .map_or(TrafficBeat::SILENT, |f| TrafficBeat::from(&f.0));
    let drive = match (features.as_deref(), show.as_deref()) {
        (Some(f), s) => TrafficDrive {
            drop: s.is_some_and(|s| s.0.drop_now),
            intensity: f.0.intensity,
            section: f.0.section,
        },
        (None, _) => TrafficDrive::START,
    };
    let level = show.as_deref().map(|s| s.0.level);
    if level != state.level {
        state.level = level;
        state.level_seq = state.level_seq.wrapping_add(1);
    }
    let altitude = animation.altitude;
    let target = if animation.airborne() {
        floor_altitude(i32::from(animation.floor)) as f32
    } else {
        0.0
    };
    let cycle = palette
        .as_deref()
        .map_or([LinearRgba::WHITE; 6], |p| p.prismatic_cycle().map(linear));
    let named = palette
        .as_deref()
        .map_or([LinearRgba::WHITE; SceneColor::ALL.len()], |p| {
            SceneColor::ALL.map(|c| linear(p.get(c)))
        });
    let title = signs.names.as_deref().and_then(dj_title);
    let frame = TrafficFrame {
        dt,
        time: now,
        car: car.0,
        altitude,
        target,
        speed: state.speed,
        beat,
        drive,
        dance_beat: f64::from(beat.count),
        playing: true,
        calm: settings.calm,
        show: show
            .as_deref()
            .map(|s| TrafficShow::of(&s.0, state.level_seq, settings.titles || title.is_some())),
        cycle,
        density: levers.as_deref().map_or(1.0, |l| l.particle_density),
        palette: named,
        dj_title: title,
    };

    // C'est le tour d'un véhicule quand le culling de simulation de son entité le dit ; sans entité (tests sans rendu),
    // toujours.
    let due = |craft: &super::fleet::Craft| {
        entities
            .as_deref()
            .and_then(|e| e.body(craft.variant, craft.slot))
            .and_then(|body| lods.get(body).ok())
            .is_none_or(SimulationLod::due)
    };
    // Le chef du spectacle quand il existe ; sinon, personne : tout est permis sauf les événements du spectacle seul.
    let mut runtime_link;
    let mut no_show = NoShow;
    let link: &mut dyn ShowLink = match runtime.as_deref_mut() {
        Some(runtime) => {
            runtime_link = RuntimeLink(runtime);
            &mut runtime_link
        }
        None => &mut no_show,
    };
    let lender = signs.road.as_deref_mut().map(|road| &mut road.0 as &mut dyn SignLender);
    director.update_with_signs(&frame, &mut registry, link, lender, due);

    let next = TrafficStats {
        fleet: director.fleet.stats(),
        ground: director.ground.count(),
        events: director.active_count(),
        glints: director.glint_count(),
        walls: director.walls.active(),
        entities: entities.as_deref().map_or(0, TrafficEntities::entity_count),
    };
    stats.set_if_neq(next);
}

/// `CfSet::Simulate`, après le pas : transmet à la ville les zones que les événements réclament, quand elles changent.
pub(crate) fn apply_clearings(
    mut director: ResMut<TrafficDirector>,
    world: Option<ResMut<NeonWorld>>,
    stream: Option<ResMut<ChunkStream<NeonWorld>>>,
) {
    let Some(clearings) = director.take_clearings() else {
        return;
    };
    let (Some(mut world), Some(mut stream)) = (world, stream) else {
        return;
    };
    if world.set_clearings(clearings) {
        stream.replan_all();
    }
}

/// `CfSet::Present` : les limiteurs de flashs du trafic (règle 7), avec la même politique que la voiture. Ils reçoivent
/// aussi les demandes des événements (éclair, salve, variation de lumière de la ville) et rendent les niveaux accordés.
pub(crate) fn update_flash(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShowFx>>,
    mut flash: ResMut<TrafficFlash>,
    mut director: ResMut<TrafficDirector>,
) {
    let policy = show.map(|s| (s.0.flashes, s.0.breathing));
    let pulse = features.map_or(0.0, |f| f.0.pulse);
    let now = time.elapsed_secs_f64();
    flash.update(time.delta_secs(), now, policy, pulse);
    flash.serve(now, &mut director.scene.fx);
}

/// Part du blanchissement de la ville qu'un éclair accordé peut atteindre : `min(1, flash × 0,9)` de Sillage, le niveau
/// accordé étant déjà borné par le limiteur.
const LIGHTNING_GAIN: f32 = 0.9;

/// `CfSet::Present`, après les limiteurs : ce que les événements font de la ville (éclair, panne, pouls des fenêtres),
/// écrit dans `FacadeLook`. L'éclair est le niveau que le limiteur accorde, jamais la demande de l'événement.
pub(crate) fn publish_city_fx(
    director: Res<TrafficDirector>,
    origin: Option<Res<FloatingOrigin>>,
    mut look: ResMut<FacadeLook>,
) {
    let fx = director.scene.fx;
    let car_z = director.scene.panels.origin().z;
    // Le front est relatif à la voiture ; la façade le lit en coordonnées du monde rendu.
    let power = if fx.power.depth > 0.0 {
        let front = DVec3::new(0.0, 0.0, car_z + f64::from(fx.power.front_rel_m));
        let render_z = origin.as_deref().map_or(front.z, |o| f64::from(o.to_render(front).z));
        Vec4::new(render_z as f32, fx.power.edge_m, fx.power.depth, -1.0)
    } else {
        NO_BLACKOUT
    };
    look.set_if_neq(FacadeLook {
        flash: (fx.lightning * LIGHTNING_GAIN).clamp(0.0, 1.0),
        win_pulse: fx.win_pulse.clamp(0.0, 1.0),
        power,
        ..*look
    });
}
