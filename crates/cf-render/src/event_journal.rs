//! Le journal des demandes d'événements : ce que l'opérateur (ou le MIDI) a demandé, et ce que le monde en a fait.
//!
//! Avant ce journal, un déclencheur refusé (place prise, pont tout proche, événement impossible à construire) disparaissait
//! sans trace : l'opérateur voyait « rien ne se passe » sans savoir pourquoi. Chaque demande est maintenant inscrite en
//! attente, puis **résolue** par le directeur qui la prend : démarrée, ou refusée avec une raison lisible. Une demande que
//! personne ne prend dans le délai d'un ordre (`ORDER_TTL`) est marquée « non prise ».
//!
//! Le journal est borné ([`CAPACITY`]) : il ne grossit jamais, et les écritures n'arrivent qu'à un geste ou à un
//! lancement, jamais à chaque image.

use std::collections::VecDeque;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use cf_control::{Action, TimedAction, TriggerId};

use crate::CfSet;
use crate::active_events::EventOrigin;
use crate::controls::ControlAction;
use crate::sim::{SimClock, SimControl};

/// Nombre d'entrées conservées ; les plus anciennes sont oubliées.
pub const CAPACITY: usize = 64;

/// Une demande que personne n'a prise au bout de ce temps (s de simulation) est « non prise » (`ORDER_TTL` + marge).
pub const UNCLAIMED_AFTER_S: f64 = 3.5;

/// Pourquoi le monde n'a pas lancé un événement demandé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    /// Le même événement est déjà en cours.
    AlreadyRunning,
    /// Une structure occupe déjà la route : deux structures ne se superposent jamais.
    StructureBusy,
    /// Un événement de ciel est déjà en cours (un seul à la fois).
    SkyBusy,
    /// Le titre holographique et l'aurore se disputent le haut du ciel.
    SkyClash,
    /// Le nombre maximal d'événements simultanés est atteint.
    TooManyParallel,
    /// Une arche-immeuble est trop proche pour poser une structure.
    NearBridge,
    /// Le monde n'a pas su construire l'événement (famille absente, ressource manquante).
    NotBuildable,
    /// Une cascade de la voiture a la ville pour elle.
    Held,
    /// La simulation est en pause : personne ne traite les demandes.
    Paused,
    /// Aucun directeur n'a reconnu cette demande.
    Unclaimed,
    /// L'événement a attendu une place trop longtemps.
    Timeout,
}

impl RefusalReason {
    /// La raison, en une phrase pour l'opérateur.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::AlreadyRunning => "déjà en cours",
            Self::StructureBusy => "une structure occupe déjà la route",
            Self::SkyBusy => "un autre événement de ciel est en cours",
            Self::SkyClash => "conflit titre holographique / aurore",
            Self::TooManyParallel => "trop d'événements en même temps",
            Self::NearBridge => "une arche est trop proche",
            Self::NotBuildable => "le monde n'a pas pu le construire",
            Self::Held => "une cascade tient la ville",
            Self::Paused => "simulation en pause",
            Self::Unclaimed => "personne n'a pris la demande",
            Self::Timeout => "aucune place avant le délai",
        }
    }
}

/// Où en est une demande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalState {
    /// Faite pendant la pause : armée, elle part toute seule à la reprise de la simulation.
    Armed,
    /// Déposée, pas encore prise.
    Pending,
    /// Lancée par le monde.
    Started,
    /// Refusée, avec la raison.
    Refused(RefusalReason),
}

/// Une ligne du journal.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    /// Numéro d'ordre, qui ne fait que monter.
    pub seq: u64,
    /// Temps de simulation (s) de la demande ou du lancement.
    pub at_s: f64,
    /// Identifiant demandé ou lancé (`"chase"`, `"landmark:arcade"`).
    pub event_id: String,
    /// Qui a lancé (la demande d'un opérateur est toujours `Manual`).
    pub origin: EventOrigin,
    /// État.
    pub state: JournalState,
}

/// Deux identifiants parlent du même événement : égaux, ou de même famille avant le `:` (`landmark:arcade` et `landmark`).
fn same_event(requested: &str, started: &str) -> bool {
    requested == started || requested.split(':').next() == started.split(':').next()
}

/// Le journal borné des demandes et des lancements.
#[derive(Resource, Debug)]
pub struct EventJournal {
    entries: VecDeque<JournalEntry>,
    next_seq: u64,
}

impl Default for EventJournal {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(CAPACITY),
            next_seq: 0,
        }
    }
}

impl EventJournal {
    fn push(&mut self, at_s: f64, event_id: &str, origin: EventOrigin, state: JournalState) {
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(JournalEntry {
            seq: self.next_seq,
            at_s,
            event_id: event_id.to_owned(),
            origin,
            state,
        });
        self.next_seq += 1;
    }

    /// Passe en attente les demandes armées (la simulation reprend : elles vont être prises).
    pub fn release_armed(&mut self, at_s: f64) {
        for entry in &mut self.entries {
            if entry.state == JournalState::Armed {
                entry.state = JournalState::Pending;
                entry.at_s = at_s;
            }
        }
    }

    /// Une demande armée est refusée (file d'armement pleine).
    pub fn request_armed(&mut self, at_s: f64, event_id: &str) {
        self.push(at_s, event_id, EventOrigin::Manual, JournalState::Armed);
    }

    /// La plus ancienne demande en attente qui vise cet événement.
    fn pending_for(&mut self, event_id: &str) -> Option<&mut JournalEntry> {
        self.entries
            .iter_mut()
            .find(|e| e.state == JournalState::Pending && same_event(&e.event_id, event_id))
    }

    /// Une demande de l'opérateur vient d'être déposée.
    pub fn request(&mut self, at_s: f64, event_id: &str) {
        self.push(at_s, event_id, EventOrigin::Manual, JournalState::Pending);
    }

    /// Le monde a lancé cet événement : résout la demande en attente, ou ajoute une ligne (lancement spontané).
    pub fn started(&mut self, at_s: f64, event_id: &str, origin: EventOrigin) {
        if origin == EventOrigin::Manual
            && let Some(entry) = self.pending_for(event_id)
        {
            entry.state = JournalState::Started;
            entry.at_s = at_s;
            return;
        }
        self.push(at_s, event_id, origin, JournalState::Started);
    }

    /// Le monde a refusé cet événement : résout la demande en attente, ou ajoute une ligne.
    pub fn refused(&mut self, at_s: f64, event_id: &str, reason: RefusalReason) {
        if let Some(entry) = self.pending_for(event_id) {
            entry.state = JournalState::Refused(reason);
            return;
        }
        self.push(at_s, event_id, EventOrigin::Manual, JournalState::Refused(reason));
    }

    /// Marque « refusée » toute demande en attente déposée depuis plus de `after_s` secondes avant `now_s`.
    pub fn expire_pending(&mut self, now_s: f64, after_s: f64, reason: RefusalReason) {
        for entry in &mut self.entries {
            if entry.state == JournalState::Pending && now_s - entry.at_s >= after_s {
                entry.state = JournalState::Refused(reason);
            }
        }
    }

    /// Les lignes, de la plus ancienne à la plus récente.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &JournalEntry> {
        self.entries.iter()
    }

    /// Nombre de lignes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Le journal est vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Nombre de demandes en attente.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.entries.iter().filter(|e| e.state == JournalState::Pending).count()
    }

    /// Efface tout (bouton « Vider » ou reset de la simulation).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Nombre de demandes armées pendant une pause, au plus (la file de déclencheurs du chef du spectacle en tient autant).
pub const ARMED_MAX: usize = 8;

/// Les demandes faites pendant la pause, dans l'ordre. À la reprise, elles partent toutes ensemble : c'est ce qui permet
/// de composer un plateau (un immeuble, des motos, une poursuite…) à l'arrêt, puis de le lancer d'un coup.
#[derive(Resource, Debug, Default)]
pub struct ArmedTriggers {
    ids: Vec<TriggerId>,
    /// Déclencheurs que nous venons de réémettre : le prochain passage ne doit pas les journaliser une seconde fois.
    reemitted: usize,
}

impl ArmedTriggers {
    /// Nombre de demandes armées.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Rien d'armé.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Abandonne les demandes armées (bouton « Tout nettoyer » pendant la pause).
    pub fn clear(&mut self) {
        self.ids.clear();
    }
}

/// Inscrit chaque `Action::Trigger` reçu. Tourne en `Input`, donc même en pause. Pendant la pause, personne ne traite les
/// demandes (le chef du spectacle dort et les messages expirent) : elles sont **armées** et réémises toutes ensemble à la
/// reprise, au lieu de se perdre en silence.
pub fn record_requests(
    mut actions: ResMut<Messages<ControlAction>>,
    mut cursor: Local<MessageCursor<ControlAction>>,
    clock: Res<SimClock>,
    time: Res<Time>,
    control: Res<SimControl>,
    mut journal: ResMut<EventJournal>,
    mut armed: ResMut<ArmedTriggers>,
    mut timeline: ResMut<crate::DebugTimeline>,
) {
    let now = clock.sim_s();
    let mut skip = std::mem::take(&mut armed.reemitted);
    let mut seen: Vec<TriggerId> = Vec::new();
    for action in cursor.read(&actions) {
        if let Action::Trigger(id) = &action.action {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            seen.push(id.clone());
        }
    }
    for id in seen {
        if control.paused() {
            if armed.ids.len() < ARMED_MAX {
                journal.request_armed(now, &id.0);
                armed.ids.push(id);
            } else {
                journal.request(now, &id.0);
                journal.refused(now, &id.0, RefusalReason::TooManyParallel);
            }
        } else {
            journal.request(now, &id.0);
            timeline.request(now, &id.0);
        }
    }
    if !control.paused() && !armed.ids.is_empty() {
        let ids = std::mem::take(&mut armed.ids);
        armed.reemitted = ids.len();
        journal.release_armed(now);
        for id in ids {
            timeline.request(now, &id.0);
            actions.write(ControlAction(TimedAction {
                time_s: time.elapsed_secs_f64(),
                action: Action::Trigger(id),
            }));
        }
    }
}

/// Marque « non prise » une demande que personne n'a résolue dans le délai.
pub fn expire_unclaimed(clock: Res<SimClock>, mut journal: ResMut<EventJournal>) {
    if journal.pending() > 0 {
        journal.expire_pending(clock.sim_s(), UNCLAIMED_AFTER_S, RefusalReason::Unclaimed);
    }
}

/// Enregistre [`EventJournal`] et ses deux systèmes.
#[derive(Debug, Default)]
pub struct EventJournalPlugin;

impl Plugin for EventJournalPlugin {
    fn build(&self, app: &mut App) {
        // Idempotent : sans contrôles (tests, `--no-operator`), le lecteur de messages a quand même sa file.
        app.add_message::<ControlAction>()
            .init_resource::<EventJournal>()
            .init_resource::<ArmedTriggers>()
            .add_systems(Update, record_requests.in_set(CfSet::Input))
            .add_systems(Update, expire_unclaimed.in_set(CfSet::Simulate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_resolved_by_the_matching_start() {
        let mut journal = EventJournal::default();
        journal.request(1.0, "landmark:arcade");
        journal.started(1.5, "landmark", EventOrigin::Manual);
        let lines: Vec<_> = journal.iter().collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].state, JournalState::Started);
        assert!((lines[0].at_s - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_spontaneous_start_adds_its_own_line_and_keeps_the_pending_request() {
        let mut journal = EventJournal::default();
        journal.request(1.0, "chase");
        journal.started(2.0, "chase", EventOrigin::Auto);
        assert_eq!(journal.len(), 2);
        assert_eq!(journal.pending(), 1);
    }

    #[test]
    fn a_refusal_keeps_its_reason() {
        let mut journal = EventJournal::default();
        journal.request(1.0, "tower");
        journal.refused(1.1, "tower", RefusalReason::StructureBusy);
        assert_eq!(
            journal.iter().next().map(|e| e.state),
            Some(JournalState::Refused(RefusalReason::StructureBusy))
        );
        assert_eq!(journal.pending(), 0);
    }

    #[test]
    fn nobody_claiming_a_request_marks_it_unclaimed() {
        let mut journal = EventJournal::default();
        journal.request(1.0, "mystère");
        journal.expire_pending(2.0, UNCLAIMED_AFTER_S, RefusalReason::Unclaimed);
        assert_eq!(journal.pending(), 1, "pas encore");
        journal.expire_pending(5.0, UNCLAIMED_AFTER_S, RefusalReason::Unclaimed);
        assert_eq!(journal.pending(), 0);
        assert_eq!(
            journal.iter().next().map(|e| e.state),
            Some(JournalState::Refused(RefusalReason::Unclaimed))
        );
    }

    #[test]
    fn the_journal_is_bounded() {
        let mut journal = EventJournal::default();
        for i in 0..(CAPACITY * 2) {
            journal.started(i as f64, "chase", EventOrigin::Auto);
        }
        assert_eq!(journal.len(), CAPACITY);
        assert_eq!(journal.iter().next().map(|e| e.seq), Some(CAPACITY as u64));
    }
}
