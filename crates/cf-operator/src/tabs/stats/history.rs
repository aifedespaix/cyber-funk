//! Ce que l'onglet Statistiques accumule au fil de la session : événements lancés par famille, énergie audio des
//! 60 dernières secondes, temps d'antenne par créneau DJ. Tout est mesuré ici, à partir de ressources existantes ;
//! rien n'est inventé ni lu dans le monde.
//!
//! Les compteurs partent de l'ouverture de la fenêtre opérateur. Aucune allocation en régime établi (anneau de taille
//! fixe, tampons réutilisés ; le tableau des créneaux ne change de taille qu'avec la feuille de conduite).

use bevy::prelude::*;
use cf_render::audio::AudioFeatures;
use cf_render::{ActiveEvents, ActiveId, CurrentShow, EventCategory, ShowSheet};

/// Période d'échantillonnage de l'énergie, en secondes : 4 points par seconde suffisent à une courbe de 60 s.
const ENERGY_PERIOD_S: f64 = 0.25;
/// Points gardés : 60 s à 4 points par seconde.
pub(crate) const ENERGY_SAMPLES: usize = 240;

/// Événements lancés depuis l'ouverture, par famille. Un événement est compté à sa première apparition dans
/// [`ActiveEvents`] (un événement plus court qu'une image n'y est jamais vu : il n'est pas compté).
#[derive(Resource, Debug, Default)]
pub(crate) struct EventTally {
    counts: [u32; EventCategory::ALL.len()],
    seen: Vec<ActiveId>,
    scratch: Vec<ActiveId>,
}

impl EventTally {
    /// Compte les événements apparus depuis le dernier appel.
    pub(crate) fn observe(&mut self, events: &ActiveEvents) {
        self.scratch.clear();
        for event in events.iter() {
            self.scratch.push(event.id);
            if !self.seen.contains(&event.id)
                && let Some(i) = EventCategory::ALL.iter().position(|c| *c == event.category)
            {
                self.counts[i] = self.counts[i].saturating_add(1);
            }
        }
        std::mem::swap(&mut self.seen, &mut self.scratch);
    }

    /// Événements de cette famille lancés depuis l'ouverture.
    pub(crate) fn count(&self, category: EventCategory) -> u32 {
        EventCategory::ALL
            .iter()
            .position(|c| *c == category)
            .map_or(0, |i| self.counts[i])
    }

    /// Tous les événements lancés depuis l'ouverture.
    pub(crate) fn total(&self) -> u32 {
        self.counts.iter().copied().fold(0, u32::saturating_add)
    }
}

/// Volume global (`AudioFeatures::loud`, 0..1) des 60 dernières secondes, en anneau.
#[derive(Resource, Debug)]
pub(crate) struct EnergyHistory {
    samples: [f32; ENERGY_SAMPLES],
    /// Rang du prochain point écrit.
    head: usize,
    len: usize,
    /// Prochain échantillon dû (temps réel, s).
    next_at_s: f64,
}

impl Default for EnergyHistory {
    fn default() -> Self {
        Self {
            samples: [0.0; ENERGY_SAMPLES],
            head: 0,
            len: 0,
            next_at_s: 0.0,
        }
    }
}

impl EnergyHistory {
    /// Ajoute un point (le plus ancien est oublié quand l'anneau est plein).
    pub(crate) fn push(&mut self, value: f32) {
        self.samples[self.head] = value;
        self.head = (self.head + 1) % ENERGY_SAMPLES;
        self.len = (self.len + 1).min(ENERGY_SAMPLES);
    }

    /// Nombre de points gardés.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Les points, du plus ancien au plus récent.
    pub(crate) fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        let start = (self.head + ENERGY_SAMPLES - self.len) % ENERGY_SAMPLES;
        (0..self.len).map(move |i| self.samples[(start + i) % ENERGY_SAMPLES])
    }
}

/// Temps passé à l'écran par chaque créneau de la feuille de conduite, en secondes de temps réel.
#[derive(Resource, Debug, Default)]
pub(crate) struct Airtime {
    /// Une entrée par créneau, dans l'ordre de la feuille.
    pub secs: Vec<f64>,
}

/// Branche les compteurs. Ajouté par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.init_resource::<EventTally>()
        .init_resource::<EnergyHistory>()
        .init_resource::<Airtime>()
        .add_systems(
            Update,
            (tally_events, sample_energy, count_airtime).in_set(cf_render::CfSet::Present),
        );
}

fn tally_events(events: Option<Res<ActiveEvents>>, mut tally: ResMut<EventTally>) {
    if let Some(events) = events
        && events.is_changed()
    {
        tally.observe(&events);
    }
}

fn sample_energy(time: Res<Time<Real>>, features: Option<Res<AudioFeatures>>, mut history: ResMut<EnergyHistory>) {
    let Some(features) = features else {
        return;
    };
    let now_s = time.elapsed_secs_f64();
    if now_s < history.next_at_s {
        return;
    }
    history.next_at_s = now_s + ENERGY_PERIOD_S;
    history.push(features.loud.clamp(0.0, 1.0));
}

fn count_airtime(
    time: Res<Time<Real>>,
    show: Option<Res<CurrentShow>>,
    sheet: Option<Res<ShowSheet>>,
    mut airtime: ResMut<Airtime>,
) {
    let (Some(show), Some(sheet)) = (show, sheet) else {
        return;
    };
    if airtime.secs.len() != sheet.slots.len() {
        airtime.secs.resize(sheet.slots.len(), 0.0);
    }
    if let Some(secs) = airtime.secs.get_mut(show.0.slot) {
        *secs += time.delta_secs_f64();
    }
}

#[cfg(test)]
mod tests {
    use cf_render::{ActiveEvent, EventOrigin};

    use super::*;

    fn event(category: EventCategory) -> ActiveEvent {
        ActiveEvent {
            id: ActiveId::UNSET,
            event_id: "test",
            label: "Test".to_owned(),
            category,
            origin: EventOrigin::Auto,
            started_s: 0.0,
            duration_s: None,
        }
    }

    #[test]
    fn an_event_is_counted_once_when_it_appears() {
        let mut events = ActiveEvents::default();
        let mut tally = EventTally::default();
        let sign = events.begin(event(EventCategory::Sign));
        tally.observe(&events);
        tally.observe(&events);
        assert_eq!(tally.count(EventCategory::Sign), 1, "toujours le même événement");
        events.begin(event(EventCategory::Car));
        events.end(sign);
        events.begin(event(EventCategory::Sign));
        tally.observe(&events);
        assert_eq!(tally.count(EventCategory::Sign), 2);
        assert_eq!(tally.count(EventCategory::Car), 1);
        assert_eq!(tally.total(), 3);
    }

    #[test]
    fn energy_ring_keeps_the_last_points_in_order() {
        let mut history = EnergyHistory::default();
        for i in 0..(ENERGY_SAMPLES + 3) {
            history.push(i as f32);
        }
        assert_eq!(history.len(), ENERGY_SAMPLES);
        let points: Vec<f32> = history.iter().collect();
        assert!((points[0] - 3.0).abs() < f32::EPSILON);
        assert!((points[ENERGY_SAMPLES - 1] - (ENERGY_SAMPLES + 2) as f32).abs() < f32::EPSILON);
    }
}
