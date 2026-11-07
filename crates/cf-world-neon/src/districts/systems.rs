//! Les systèmes du moteur de quartiers : démarrage, déclencheurs `district:<type>`, tick du gouverneur.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use cf_analysis::Section;
use cf_render::{
    AbsolutePosition, AudioFeatures, ChunkStream, CurrentShow, DistrictGradeState, DistrictOrder, DistrictState,
    EventCategory, QualityLevers, ShowRuntime, StreamFocus, World as _,
};
use cf_show::district::{DistrictId, DistrictKind, Governor, TransitionKind};

use super::kind_of;
use super::state::{Change, DistrictBrain, DistrictDriver, DistrictInput};
use crate::automation::{Automation, Launch, origin};
use crate::facade::{LOW_QUALITY_LEVEL, LowQualityMix};
use crate::scenery::LEAD_ROWS;
use crate::traffic::TrafficDirector;
use crate::world::{DEFAULT_SEED, NeonWorld};

/// Rangée de marge ajoutée à [`LEAD_ROWS`] : la voiture peut encore avancer d'une rangée entre ce système et le
/// streaming de la même image (le pilote n'est pas ordonné avant nous), le segment reste au-delà de toute rangée
/// générée.
const MARGIN_ROWS: i64 = 1;

/// `Startup` : crée le gouverneur (graine du monde, nombre de variantes de chaque type d'après les étalonnages) et
/// active les quartiers dans la génération. Le plan par défaut est « centre néon partout » : la ville générée ne
/// change pas tant qu'aucun changement n'est décidé, rien à re-tirer.
pub fn start_districts(
    mut commands: Commands,
    world: Option<ResMut<NeonWorld>>,
    grades: Option<Res<DistrictGradeState>>,
) {
    let seed = world.as_deref().map_or(DEFAULT_SEED, NeonWorld::seed);
    if let Some(mut world) = world {
        world.set_districts_active(true);
    }
    let variants = DistrictKind::ALL.map(|kind| grades.as_deref().map_or(1, |g| g.library().variants(kind)));
    commands.insert_resource(DistrictBrain(Governor::new(seed, variants)));
}

/// `CfSet::Simulate`, avant le tick : prend un déclencheur `district:<type>` (`Action::Trigger`, chemin du MIDI et du
/// catalogue) et le relaie en [`DistrictOrder::Force`] par dégradé. La variante est tirée (graine du monde et rang du
/// déclencheur), jamais celle que ce type a eue en dernier. Un autre identifiant reste dans la file.
pub fn take_district_triggers(
    runtime: Option<ResMut<ShowRuntime>>,
    brain: Res<DistrictBrain>,
    world: Option<Res<NeonWorld>>,
    grades: Option<Res<DistrictGradeState>>,
    mut orders: MessageWriter<DistrictOrder>,
    mut drawn: Local<u64>,
) {
    let Some(mut runtime) = runtime else { return };
    let wanted = runtime
        .director()
        .state()
        .orders
        .trigger_pending()
        .and_then(|id| kind_of(&id.0));
    let Some(kind) = wanted else { return };
    if runtime.director_mut().take_trigger().is_none() {
        return;
    }
    *drawn += 1;
    let count = grades.as_deref().map_or(1, |g| g.library().variants(kind)).max(1);
    let last = brain
        .0
        .history()
        .iter()
        .rev()
        .find(|id| id.kind == kind)
        .map(|id| id.variant);
    let seed = world.as_deref().map_or(DEFAULT_SEED, NeonWorld::seed);
    let variant = pick_variant(count, last, mix(seed ^ mix(*drawn)));
    orders.write(DistrictOrder::Force {
        to: DistrictId { kind, variant },
        transition: TransitionKind::Gradient,
    });
}

/// `CfSet::Simulate` : en qualité basse (niveau ≥ [`LOW_QUALITY_LEVEL`], celui des façades), l'étalonnage de quartier
/// glisse vers le neutre en [`LOW_QUALITY_FADE_S`](crate::facade::LOW_QUALITY_FADE_S) (rampe linéaire de
/// [`LowQualityMix`], jamais d'à-coup : règle 7), et en revient de même quand la qualité remonte. La ville continue de
/// changer : la géométrie ne coûte rien de plus, seul l'étalonnage (un calcul de palette par image) est retiré. Rien n'est écrit tant que le repli ne bouge pas (pas de
/// détection de changement inutile).
pub fn fade_grade_for_quality(
    time: Res<Time>,
    quality: Option<Res<QualityLevers>>,
    grade: Option<ResMut<DistrictGradeState>>,
    mut low: Local<LowQualityMix>,
) {
    let Some(mut grade) = grade else { return };
    let mix = low.update(time.delta_secs(), quality.is_some_and(|q| q.level >= LOW_QUALITY_LEVEL));
    // Comparaison exacte (bits) : la rampe se pose exactement sur 0 et 1, et le moindre pas doit passer.
    if grade.low_quality_mix().to_bits() != mix.to_bits() {
        grade.set_low_quality_mix(mix);
    }
}

/// Une variante parmi `count`, autre que `last` quand il y a le choix.
fn pick_variant(count: u8, last: Option<u8>, draw: u64) -> u8 {
    let excluded = last.filter(|&v| v < count);
    let choices = u64::from(count) - u64::from(excluded.is_some() && count > 1);
    let mut k = (draw % choices.max(1)) as u8;
    if let Some(v) = excluded
        && count > 1
        && k >= v
    {
        k += 1;
    }
    k
}

/// Mélangeur `SplitMix64`.
fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Ce que le tick lit du reste de l'application.
#[allow(missing_debug_implementations)] // Paramètre système : des accès aux ressources, rien à afficher.
#[derive(SystemParam)]
pub struct DistrictSources<'w, 's> {
    time: Res<'w, Time>,
    features: Option<Res<'w, AudioFeatures>>,
    show: Option<Res<'w, CurrentShow>>,
    runtime: Option<Res<'w, ShowRuntime>>,
    traffic: Option<Res<'w, TrafficDirector>>,
    car: Query<'w, 's, &'static AbsolutePosition, With<StreamFocus>>,
    stream: Option<Res<'w, ChunkStream<NeonWorld>>>,
}

impl DistrictSources<'_, '_> {
    fn input(&self) -> DistrictInput {
        let audio = self.features.as_deref().map(|f| f.0);
        DistrictInput {
            now_s: audio.map_or_else(|| self.time.elapsed_secs_f64(), |a| a.time_s),
            dt_s: self.time.delta_secs(),
            section: audio.map_or(Section::Calm, |a| a.section),
            section_changed: audio.is_some_and(|a| a.section_changed),
            intensity: audio.map_or(0.0, |a| a.intensity),
            bass: audio.map_or(0.0, |a| a.bass),
            high: audio.map_or(0.0, |a| a.high),
            bpm: audio.and_then(|a| a.bpm),
            drop_imminent: self
                .runtime
                .as_deref()
                .is_some_and(|r| r.director().drop_plan().is_some()),
            drop_now: self.show.as_deref().is_some_and(|s| s.0.drop_now),
            show_present: self.show.is_some(),
            active_events: self.traffic.as_deref().map(TrafficDirector::active_count),
            car_z: self
                .car
                .single()
                .ok()
                .map(|p| p.0.z)
                .or_else(|| self.stream.as_deref().map(|s| s.focus().z)),
        }
    }
}

/// `CfSet::Simulate`, avant les tronçons : applique les ordres, fait tourner le gouverneur, lance les transitions
/// décidées (segment dans le plan ; l'étalonnage part quand la voiture entre dans la zone de dégradé), retire du plan
/// les segments passés et publie [`DistrictState`].
///
/// La dernière rangée générée est le bord avant de la fenêtre de streaming : `chunk_at(−z) + AHEAD` pour la position de
/// la voiture (celle que le streaming de cette image utilise) et pour le dernier point suivi du streaming, la plus
/// grande des deux. Le segment commence [`LEAD_ROWS`] + [`MARGIN_ROWS`] plus loin : aucune rangée vivante ne change,
/// donc jamais de `replan_all`.
pub fn tick_districts(
    sources: DistrictSources,
    mut orders: MessageReader<DistrictOrder>,
    mut brain: ResMut<DistrictBrain>,
    mut driver: ResMut<DistrictDriver>,
    mut state: ResMut<DistrictState>,
    mut world: Option<ResMut<NeonWorld>>,
    mut grade: Option<ResMut<DistrictGradeState>>,
    mut automation: Automation,
) {
    // Mode manuel : le gouverneur ne décide plus rien ; les forçages (déclencheurs `district:<type>`) passent.
    let manual = !automation.may_spawn();
    if driver.manual() != manual {
        driver.set_manual(manual);
    }
    let input = sources.input();
    let (last_generated, first_live) = match (world.as_deref(), input.car_z) {
        (Some(world), Some(z)) => {
            let layout = world.layout();
            let row = layout.chunk_at(-z);
            let streamed = sources.stream.as_deref().map_or(row, |s| layout.chunk_at(-s.focus().z));
            (
                Some(row.max(streamed).saturating_add(i64::from(layout.ahead))),
                Some(row.saturating_sub(i64::from(layout.behind))),
            )
        }
        _ => (None, None),
    };

    let mut next = *state;
    // L'étalonnage ne part pas ici : il attend que la voiture entre dans la zone de dégradé (`DistrictDriver::tick`).
    let mut start = |change: Change, driver: &mut DistrictDriver, next: &mut DistrictState| {
        let from = last_generated.and_then(|last| {
            let from = last.saturating_add(LEAD_ROWS + MARGIN_ROWS);
            let world = world.as_deref_mut()?;
            world
                .push_district(last, from, change.to, change.blend_rows)
                .then_some(from)
        });
        driver.begin(&change, from, next);
    };

    for order in orders.read() {
        if let Some(change) = driver.apply_order(&mut brain.0, *order, input.now_s, &mut next) {
            start(change, &mut driver, &mut next);
        }
    }
    if let Some(change) = driver.tick(&mut brain.0, &input, grade.as_deref_mut(), &mut next) {
        start(change, &mut driver, &mut next);
    }

    // Les segments sortis de la fenêtre par l'arrière ne servent plus : on les retire (lecture d'abord, pour ne pas
    // marquer le monde modifié à chaque image).
    if let (Some(first), Some(world)) = (first_live, world.as_mut())
        && world.scenery().districts().prunable(first)
    {
        world.prune_districts(first);
    }
    state.set_if_neq(next);

    // La transition lancée est au registre des événements en cours jusqu'à sa fin réelle.
    let running = driver.running();
    automation.track(driver.changes(), running.is_some(), || {
        let (to, forced) = running.unwrap_or((next.current, true));
        Launch {
            event_id: super::trigger_id(to.kind),
            label: to.kind.label(),
            category: EventCategory::District,
            origin: origin(!forced),
            duration_s: None,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_variante_tiree_n_est_jamais_la_derniere() {
        for draw in 0..64 {
            for last in 0..6 {
                let v = pick_variant(6, Some(last), mix(draw));
                assert!(v < 6);
                assert_ne!(v, last);
            }
            assert_eq!(pick_variant(1, Some(0), mix(draw)), 0);
            assert!(pick_variant(3, None, mix(draw)) < 3);
        }
    }
}
