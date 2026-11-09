//! Registre des événements du monde en cours, avec leur progression, pour l'interface opérateur.

use bevy::prelude::*;

use crate::CfSet;
use crate::sim::SimClock;

/// Famille d'un événement du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    /// Enseignes.
    Sign,
    /// Immeubles.
    Building,
    /// Voitures.
    Car,
    /// Quartiers.
    District,
}

impl EventCategory {
    /// Toutes les familles, dans l'ordre d'affichage.
    pub const ALL: [Self; 4] = [Self::Sign, Self::Building, Self::Car, Self::District];
}

/// Qui a déclenché l'événement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventOrigin {
    /// L'opérateur ou le spectacle.
    Manual,
    /// Le monde, de lui-même.
    Auto,
}

/// Identifiant d'une entrée du registre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActiveId(u64);

impl ActiveId {
    /// Valeur à mettre dans [`ActiveEvent::id`] avant [`ActiveEvents::begin`], qui la remplace : sans elle, aucune autre
    /// crate ne pourrait construire un [`ActiveEvent`] (le champ de l'identifiant est privé).
    pub const UNSET: Self = Self(u64::MAX);
}

/// Un événement en cours.
#[derive(Debug, Clone)]
pub struct ActiveEvent {
    /// Identifiant, attribué par [`ActiveEvents::begin`] (la valeur fournie est ignorée).
    pub id: ActiveId,
    /// Identifiant de l'événement dans le catalogue.
    pub event_id: &'static str,
    /// Nom affiché.
    pub label: String,
    /// Famille.
    pub category: EventCategory,
    /// Déclencheur.
    pub origin: EventOrigin,
    /// Temps de simulation du début, en secondes.
    pub started_s: f64,
    /// Durée connue, en secondes ; `None` si l'événement dure jusqu'à ce qu'on l'arrête.
    pub duration_s: Option<f64>,
}

/// Événements en cours. Liste parcourue linéairement : quelques dizaines d'entrées au plus.
#[derive(Resource, Default, Debug)]
pub struct ActiveEvents {
    events: Vec<ActiveEvent>,
    next_id: u64,
}

impl ActiveEvents {
    /// Enregistre un événement et renvoie son identifiant.
    pub fn begin(&mut self, mut event: ActiveEvent) -> ActiveId {
        let id = ActiveId(self.next_id);
        self.next_id += 1;
        event.id = id;
        self.events.push(event);
        id
    }

    /// Retire un événement.
    pub fn end(&mut self, id: ActiveId) {
        self.events.retain(|event| event.id != id);
    }

    /// Retire tous les événements d'une famille.
    pub fn end_category(&mut self, category: EventCategory) {
        self.events.retain(|event| event.category != category);
    }

    /// Vide le registre.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Tous les événements en cours.
    pub fn iter(&self) -> impl Iterator<Item = &ActiveEvent> {
        self.events.iter()
    }

    /// Les événements d'une famille.
    pub fn in_category(&self, category: EventCategory) -> impl Iterator<Item = &ActiveEvent> {
        self.events.iter().filter(move |event| event.category == category)
    }

    /// Avancement entre 0 et 1 ; `None` si l'événement est inconnu ou sa durée indéterminée.
    #[must_use]
    pub fn progress(&self, id: ActiveId, now_s: f64) -> Option<f32> {
        let event = self.events.iter().find(|event| event.id == id)?;
        let duration = event.duration_s?;
        if duration <= 0.0 {
            return Some(1.0);
        }
        Some(((now_s - event.started_s) / duration).clamp(0.0, 1.0) as f32)
    }

    /// Nombre d'événements en cours.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Aucun événement en cours.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Retire les événements à durée connue qui sont écoulés.
    pub fn expire(&mut self, now_s: f64) {
        self.events.retain(|event| {
            event
                .duration_s
                .is_none_or(|duration| now_s - event.started_s < duration)
        });
    }
}

/// Fait expirer les événements avec l'horloge de simulation (arrêtée pendant la pause).
pub fn expire_active_events(clock: Res<SimClock>, mut events: ResMut<ActiveEvents>) {
    if !events.is_empty() {
        events.expire(clock.sim_s());
    }
}

/// Enregistre [`ActiveEvents`], [`WorldAutomation`](crate::WorldAutomation) et l'expiration des événements.
#[derive(Debug, Default)]
pub struct ActiveEventsPlugin;

impl Plugin for ActiveEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::event_journal::EventJournalPlugin)
            .init_resource::<crate::DebugSandbox>()
            .init_resource::<crate::DistrictAffinity>()
            .init_resource::<ActiveEvents>()
            .init_resource::<crate::WorldAutomation>()
            .add_systems(Update, expire_active_events.in_set(CfSet::Simulate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(category: EventCategory, started_s: f64, duration_s: Option<f64>) -> ActiveEvent {
        ActiveEvent {
            id: ActiveId(u64::MAX),
            event_id: "test",
            label: "Test".to_owned(),
            category,
            origin: EventOrigin::Manual,
            started_s,
            duration_s,
        }
    }

    #[test]
    fn progress_is_clamped_between_zero_and_one() {
        let mut events = ActiveEvents::default();
        let id = events.begin(event(EventCategory::Sign, 10.0, Some(4.0)));
        assert_eq!(events.progress(id, 5.0), Some(0.0));
        assert_eq!(events.progress(id, 12.0), Some(0.5));
        assert_eq!(events.progress(id, 99.0), Some(1.0));
    }

    #[test]
    fn progress_is_none_without_duration() {
        let mut events = ActiveEvents::default();
        let id = events.begin(event(EventCategory::Car, 0.0, None));
        assert_eq!(events.progress(id, 3.0), None);
    }

    #[test]
    fn expire_removes_only_finished_events_with_known_duration() {
        let mut events = ActiveEvents::default();
        events.begin(event(EventCategory::Sign, 0.0, Some(2.0)));
        events.begin(event(EventCategory::Sign, 0.0, Some(10.0)));
        events.begin(event(EventCategory::Sign, 0.0, None));
        events.expire(5.0);
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.duration_s != Some(2.0)));
    }

    #[test]
    fn end_category_keeps_other_categories() {
        let mut events = ActiveEvents::default();
        events.begin(event(EventCategory::Sign, 0.0, None));
        events.begin(event(EventCategory::Car, 0.0, None));
        events.end_category(EventCategory::Sign);
        assert_eq!(events.len(), 1);
        assert_eq!(events.in_category(EventCategory::Car).count(), 1);
    }

    #[test]
    fn begin_returns_distinct_ids() {
        let mut events = ActiveEvents::default();
        let a = events.begin(event(EventCategory::Sign, 0.0, None));
        let b = events.begin(event(EventCategory::Sign, 0.0, None));
        assert_ne!(a, b);
        events.end(a);
        assert_eq!(events.len(), 1);
    }
}
