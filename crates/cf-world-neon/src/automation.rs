//! Le mode manuel vu des directeurs du monde : qui a le droit de lancer un événement de lui-même, et le registre des
//! événements en cours ([`ActiveEvents`]) que l'interface opérateur affiche.
//!
//! Chaque directeur qui lance des événements de sa propre initiative demande [`may_spawn`] avant un lancement spontané ;
//! un déclenchement de l'opérateur (`Action::Trigger`, `DistrictOrder`, ordres « afficher le DJ », « Ascension »,
//! « événement au hasard ») ou du spectacle (changement de DJ, drop préparé, pré-intro) passe toujours. Le directeur
//! inscrit l'événement au registre **une fois le lancement accepté** par le monde (limiteur, place, permis), et l'en
//! retire à sa fin : un lancement refusé n'écrit rien.
//!
//! Tout est facultatif : sans [`WorldAutomation`], le monde est automatique ; sans [`ActiveEvents`] (tests existants,
//! `--no-operator`), rien n'est publié et les directeurs tournent comme avant.
//!
//! Ce qui n'est **pas** coupé en mode manuel, parce que ce n'est pas un événement mais la vie continue du décor : le trafic
//! de fond, les enseignes de la route (une par emplacement, comme les immeubles), les façades, le ciel et ses étoiles
//! filantes, la météo, les astres qui tombent, les projecteurs, le paquet du drop (joué par le spectacle). Voir
//! `automation_tests.rs` pour l'inventaire complet.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use cf_render::{
    ActiveEvent, ActiveEvents, ActiveId, EventCategory, EventJournal, EventOrigin, RefusalReason, SimClock,
    WorldAutomation,
};

/// Les directeurs qui lancent des événements d'eux-mêmes et lisent [`WorldAutomation`]. Un directeur ajouté ici doit
/// avoir ses trois tests dans `automation_tests.rs` (`every_known_director_is_covered` compare les deux listes).
pub const AUTOMATIC_DIRECTORS: &[&str] = &["traffic", "landmark", "stunts", "djsign", "districts"];

/// Le monde peut-il lancer un événement de lui-même ? Sans [`WorldAutomation`] : oui.
#[must_use]
pub fn may_spawn(automation: Option<&WorldAutomation>) -> bool {
    automation.is_none_or(|a| a.allows_spontaneous())
}

/// L'origine d'un lancement : `spontaneous` vrai pour le monde de lui-même.
#[must_use]
pub const fn origin(spontaneous: bool) -> EventOrigin {
    if spontaneous {
        EventOrigin::Auto
    } else {
        EventOrigin::Manual
    }
}

/// Ce que publie un directeur d'un seul événement à la fois (cascade, enseigne, quartier).
#[derive(Debug, Clone, Copy)]
pub struct Launch {
    /// Identifiant de catalogue.
    pub event_id: &'static str,
    /// Nom affiché.
    pub label: &'static str,
    /// Famille.
    pub category: EventCategory,
    /// Déclencheur.
    pub origin: EventOrigin,
    /// Durée prévue, s.
    pub duration_s: Option<f64>,
}

/// Le suivi d'un directeur d'un seul événement à la fois : le numéro du dernier lancement vu et son entrée au registre.
#[derive(Debug, Default)]
pub struct Tracked {
    seen: u32,
    id: Option<ActiveId>,
}

/// Ce qu'un directeur lit et écrit du mode manuel : la bascule, le registre et l'horloge de simulation. Tout est
/// facultatif (voir le module). Chaque système qui le prend a son propre [`Tracked`].
#[allow(missing_debug_implementations)] // Paramètre système : des accès aux ressources, rien à afficher.
#[derive(SystemParam)]
pub struct Automation<'w, 's> {
    switch: Option<Res<'w, WorldAutomation>>,
    events: Option<ResMut<'w, ActiveEvents>>,
    clock: Option<Res<'w, SimClock>>,
    journal: Option<ResMut<'w, EventJournal>>,
    tracked: Local<'s, Tracked>,
}

impl Automation<'_, '_> {
    /// Publie l'événement d'un directeur qui n'en a qu'un à la fois. `launches` monte à chaque lancement accepté ;
    /// `running` : l'événement vit encore. Un nouveau lancement remplace l'entrée du précédent ; la fin la retire.
    /// `launch` n'est appelé qu'au lancement : rien n'est calculé ni alloué les autres images.
    pub fn track(&mut self, launches: u32, running: bool, launch: impl FnOnce() -> Launch) {
        if launches != self.tracked.seen {
            self.tracked.seen = launches;
            if let Some(id) = self.tracked.id.take() {
                self.end(id);
            }
            if running {
                let l = launch();
                self.tracked.id = self.begin(l.event_id, l.label, l.category, l.origin, l.duration_s);
            }
        } else if !running && let Some(id) = self.tracked.id.take() {
            self.end(id);
        }
    }

    /// Le monde peut-il lancer un événement de lui-même ?
    #[must_use]
    pub fn may_spawn(&self) -> bool {
        may_spawn(self.switch.as_deref())
    }

    /// Le registre est là : les directeurs n'ont à suivre leurs événements que dans ce cas.
    #[must_use]
    pub fn publishing(&self) -> bool {
        self.events.is_some()
    }

    /// Inscrit un événement accepté. `None` sans registre. Alloue le libellé : une fois par lancement, jamais par image.
    pub fn begin(
        &mut self,
        event_id: &'static str,
        label: &str,
        category: EventCategory,
        origin: EventOrigin,
        duration_s: Option<f64>,
    ) -> Option<ActiveId> {
        let started_s = self.clock.as_deref().map_or(0.0, SimClock::sim_s);
        if let Some(journal) = self.journal.as_deref_mut() {
            journal.started(started_s, event_id, origin);
        }
        let events = self.events.as_deref_mut()?;
        Some(events.begin(ActiveEvent {
            id: ActiveId::UNSET,
            event_id,
            label: label.to_owned(),
            category,
            origin,
            started_s,
            duration_s,
        }))
    }

    /// Dit au journal qu'un événement demandé n'a pas été lancé, et pourquoi.
    pub fn refuse(&mut self, event_id: &str, reason: RefusalReason) {
        let at_s = self.clock.as_deref().map_or(0.0, SimClock::sim_s);
        if let Some(journal) = self.journal.as_deref_mut() {
            journal.refused(at_s, event_id, reason);
        }
    }

    /// Retire un événement fini (sans effet s'il a déjà expiré ou été retiré par l'interface).
    pub fn end(&mut self, id: ActiveId) {
        if let Some(events) = self.events.as_deref_mut() {
            events.end(id);
        }
    }
}
