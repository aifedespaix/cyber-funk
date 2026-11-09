//! La timeline de la session de debug : ce que l'opérateur a fait jouer, et quand.
//!
//! Elle enregistre deux choses sur l'horloge de simulation (la pause l'arrête) :
//!
//! - les **tronçons** : chaque événement du registre `ActiveEvents`, de son début à sa fin ;
//! - les **demandes** de l'opérateur (clics, MIDI), avec leur instant, pour **rejouer** la séquence.
//!
//! Elle part de la seconde 0 à la première demande ou au premier événement, et va jusqu'à la fin du dernier. Un bouton
//! « Tout nettoyer » la remet à zéro. Le rapport texte ([`DebugTimeline::report`]) se colle tel quel dans une demande à un
//! agent : il dit ce qui a tourné, quand, et pendant combien de temps.
//!
//! Les tampons sont bornés ([`MAX_SPANS`], [`MAX_REQUESTS`]) : une session longue ne fait pas grossir la mémoire.

use std::fmt::Write as _;

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use cf_control::{Action, TimedAction, TriggerId};

use crate::CfSet;
use crate::active_events::{ActiveEvents, ActiveId, EventCategory, EventOrigin};
use crate::controls::ControlAction;
use crate::sim::SimClock;

/// Nombre de tronçons conservés ; les plus anciens sont oubliés.
pub const MAX_SPANS: usize = 256;
/// Nombre de demandes conservées pour le rejeu.
pub const MAX_REQUESTS: usize = 64;

/// Un événement sur la timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineSpan {
    /// Identifiant de catalogue.
    pub event_id: &'static str,
    /// Nom affiché.
    pub label: String,
    /// Colonne.
    pub category: EventCategory,
    /// Qui l'a lancé.
    pub origin: EventOrigin,
    /// Début, en secondes depuis le départ de la timeline.
    pub start_s: f64,
    /// Fin, `None` tant qu'il tourne.
    pub end_s: Option<f64>,
}

/// Un plan de caméra sur la timeline, en secondes depuis le départ de la timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotSpan {
    /// Identifiant du plan (`"chase"`, `"cockpit"`…).
    pub id: &'static str,
    /// Début (≥ 0 : un plan commencé avant le départ y est ramené).
    pub start_s: f64,
    /// Fin, `None` tant que le plan est à l'antenne.
    pub end_s: Option<f64>,
}

/// Nombre maximal de plans gardés : les plus anciens partent.
pub const MAX_SHOTS: usize = 256;

/// Un plan en temps de simulation absolu (la timeline n'a d'origine qu'au premier événement).
#[derive(Debug, Clone, Copy)]
struct ShotLog {
    id: &'static str,
    start_s: f64,
    end_s: Option<f64>,
}

/// La timeline.
#[derive(Resource, Debug, Default)]
pub struct DebugTimeline {
    /// Les plans de caméra, du plus ancien au plus récent ; seul le dernier peut être ouvert.
    shots: Vec<ShotLog>,
    /// Instant de simulation (s) du départ ; `None` tant que rien ne s'est passé.
    origin_s: Option<f64>,
    spans: Vec<TimelineSpan>,
    /// Tronçons ouverts : entrée du registre et rang dans `spans`.
    open: Vec<(ActiveId, usize)>,
    /// Demandes de l'opérateur : instant relatif (s) et identifiant de déclencheur.
    requests: Vec<(f64, String)>,
}

impl DebugTimeline {
    fn relative(&mut self, now_s: f64) -> f64 {
        let origin = *self.origin_s.get_or_insert(now_s);
        (now_s - origin).max(0.0)
    }

    /// Remet la timeline à zéro (les demandes aussi).
    pub fn reset(&mut self) {
        self.origin_s = None;
        self.spans.clear();
        self.open.clear();
        self.requests.clear();
        // Le plan à l'antenne reste affiché : on le garde, rouvert à zéro au prochain suivi.
        self.shots.clear();
    }

    /// Suit la caméra : `shot` est le plan à l'antenne (`None` : aucun). Un plan qui change ferme le précédent et ouvre le
    /// suivant. Sans effet sur l'origine de la timeline (la caméra tourne bien avant le premier événement).
    pub fn track_shot(&mut self, now_s: f64, shot: Option<&'static str>) {
        let open = self
            .shots
            .last()
            .filter(|last| last.end_s.is_none())
            .map(|last| last.id);
        if open == shot {
            return;
        }
        if let Some(last) = self.shots.last_mut().filter(|last| last.end_s.is_none()) {
            last.end_s = Some(now_s.max(last.start_s));
        }
        if let Some(id) = shot {
            if self.shots.len() == MAX_SHOTS {
                self.shots.remove(0);
            }
            self.shots.push(ShotLog {
                id,
                start_s: now_s,
                end_s: None,
            });
        }
    }

    /// Le plan à l'antenne.
    #[must_use]
    pub fn current_shot(&self) -> Option<&'static str> {
        self.shots
            .last()
            .filter(|last| last.end_s.is_none())
            .map(|last| last.id)
    }

    /// Les plans de caméra depuis le départ de la timeline, ramenés à ses secondes (vide tant que rien ne s'est passé).
    pub fn shot_spans(&self) -> impl Iterator<Item = ShotSpan> + '_ {
        let origin = self.origin_s;
        self.shots.iter().filter_map(move |shot| {
            let origin = origin?;
            let start = shot.start_s - origin;
            let end = shot.end_s.map(|end| end - origin);
            // Un plan fini avant le premier événement n'a pas sa place.
            if end.is_some_and(|end| end <= 0.0) {
                return None;
            }
            Some(ShotSpan {
                id: shot.id,
                start_s: start.max(0.0),
                end_s: end,
            })
        })
    }

    /// Inscrit une demande de l'opérateur à l'instant de simulation `now_s`.
    pub fn request(&mut self, now_s: f64, trigger_id: &str) {
        let at = self.relative(now_s);
        if self.requests.len() == MAX_REQUESTS {
            self.requests.remove(0);
        }
        self.requests.push((at, trigger_id.to_owned()));
    }

    /// Suit le registre : ouvre un tronçon pour chaque nouvel événement, ferme ceux qui ont disparu.
    pub fn track(&mut self, now_s: f64, active: &ActiveEvents) {
        let mut i = self.open.len();
        while i > 0 {
            i -= 1;
            let (id, index) = self.open[i];
            if active.iter().all(|event| event.id != id) {
                let end = self.relative(now_s);
                if let Some(span) = self.spans.get_mut(index) {
                    span.end_s = Some(end.max(span.start_s));
                }
                self.open.swap_remove(i);
            }
        }
        for event in active.iter() {
            if self.open.iter().any(|(id, _)| *id == event.id) {
                continue;
            }
            // Les tronçons les plus anciens partent quand le tampon est plein : les rangs ouverts se décalent d'un cran.
            if self.spans.len() == MAX_SPANS {
                self.spans.remove(0);
                self.open.retain_mut(|(_, index)| {
                    if *index == 0 {
                        false
                    } else {
                        *index -= 1;
                        true
                    }
                });
            }
            let start = self.relative(event.started_s);
            self.spans.push(TimelineSpan {
                event_id: event.event_id,
                label: event.label.clone(),
                category: event.category,
                origin: event.origin,
                start_s: start,
                end_s: None,
            });
            self.open.push((event.id, self.spans.len() - 1));
        }
    }

    /// Les tronçons, du plus ancien au plus récent.
    #[must_use]
    pub fn spans(&self) -> &[TimelineSpan] {
        &self.spans
    }

    /// Les demandes (instant relatif en s, identifiant), pour le rejeu.
    #[must_use]
    pub fn requests(&self) -> &[(f64, String)] {
        &self.requests
    }

    /// Rien n'est enregistré.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() && self.requests.is_empty()
    }

    /// Position de la tête de lecture, en secondes depuis le départ (0 si rien n'a commencé).
    #[must_use]
    pub fn playhead_s(&self, now_s: f64) -> f64 {
        self.origin_s.map_or(0.0, |origin| (now_s - origin).max(0.0))
    }

    /// Durée de la séquence : jusqu'à la fin du dernier événement, ou jusqu'à maintenant tant que l'un tourne.
    #[must_use]
    pub fn length_s(&self, now_s: f64) -> f64 {
        let playhead = self.playhead_s(now_s);
        let ends = self
            .spans
            .iter()
            .map(|span| span.end_s.unwrap_or(playhead))
            .chain(self.requests.iter().map(|(at, _)| *at));
        ends.fold(0.0, f64::max)
    }

    /// Le rapport texte : une ligne par tronçon, de quoi le coller dans une demande à un agent.
    #[must_use]
    pub fn report(&self, now_s: f64) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Timeline de debug — {:.1} s, tête de lecture à {:.1} s",
            self.length_s(now_s),
            self.playhead_s(now_s)
        );
        for span in &self.spans {
            let end = span
                .end_s
                .map_or_else(|| "en cours".to_owned(), |end| format!("{end:.1} s"));
            let _ = writeln!(
                out,
                "  {:7.1} s → {end:<9} {} [{}] {:?} / {:?}",
                span.start_s, span.event_id, span.label, span.category, span.origin
            );
        }
        let shots: Vec<_> = self.shot_spans().collect();
        if !shots.is_empty() {
            let _ = writeln!(out, "Caméra :");
            for shot in shots {
                let end = shot
                    .end_s
                    .map_or_else(|| "à l'antenne".to_owned(), |end| format!("{end:.1} s"));
                let _ = writeln!(out, "  {:7.1} s → {end:<11} {}", shot.start_s, shot.id);
            }
        }
        if !self.requests.is_empty() {
            let _ = write!(out, "Demandes :");
            for (at, id) in &self.requests {
                let _ = write!(out, " {id}@{at:.1}s");
            }
            out.push('\n');
        }
        out
    }
}

/// Le rejeu en cours : les demandes enregistrées, rendues dans le même ordre aux mêmes instants relatifs.
#[derive(Resource, Debug, Default)]
pub struct TimelineReplay {
    plan: Vec<(f64, TriggerId)>,
    started_s: f64,
    next: usize,
    running: bool,
}

impl TimelineReplay {
    /// Lance le rejeu de ces demandes à partir de l'instant de simulation `now_s`.
    pub fn start(&mut self, now_s: f64, requests: &[(f64, String)]) {
        self.plan = requests
            .iter()
            .map(|(at, id)| (*at, TriggerId::new(id.as_str())))
            .collect();
        self.started_s = now_s;
        self.next = 0;
        self.running = !self.plan.is_empty();
    }

    /// Un rejeu est en cours.
    #[must_use]
    pub const fn running(&self) -> bool {
        self.running
    }

    /// Interrompt le rejeu.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Les demandes dues à l'instant de simulation `now_s`, dans l'ordre. Le rejeu s'arrête après la dernière.
    pub fn due(&mut self, now_s: f64) -> impl Iterator<Item = TriggerId> + '_ {
        let elapsed = now_s - self.started_s;
        let from = self.next;
        let mut to = from;
        while self.plan.get(to).is_some_and(|(at, _)| *at <= elapsed) {
            to += 1;
        }
        self.next = to;
        if to >= self.plan.len() {
            self.running = false;
        }
        self.plan[from..to].iter().map(|(_, id)| id.clone())
    }
}

/// `CfSet::Simulate`, après le monde : suit le registre des événements en cours.
pub fn track_timeline(clock: Res<SimClock>, active: Res<ActiveEvents>, mut timeline: ResMut<DebugTimeline>) {
    timeline.track(clock.sim_s(), &active);
}

/// `CfSet::Input` : émet les demandes du rejeu qui sont dues, par le chemin des contrôles (comme un clic).
pub fn run_replay(
    clock: Res<SimClock>,
    time: Res<Time>,
    mut replay: ResMut<TimelineReplay>,
    mut actions: ResMut<Messages<ControlAction>>,
) {
    if !replay.running() {
        return;
    }
    for id in replay.due(clock.sim_s()) {
        actions.write(ControlAction(TimedAction {
            time_s: time.elapsed_secs_f64(),
            action: Action::Trigger(id),
        }));
    }
}

/// Enregistre la timeline et son rejeu.
#[derive(Debug, Default)]
pub struct TimelinePlugin;

impl Plugin for TimelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ControlAction>()
            .init_resource::<DebugTimeline>()
            .init_resource::<TimelineReplay>()
            .add_systems(Update, run_replay.in_set(CfSet::Input))
            .add_systems(
                Update,
                track_timeline.in_set(CfSet::Simulate).after(crate::WorldSet::Chunks),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_events::ActiveEvent;

    fn event(id: &'static str, started_s: f64) -> ActiveEvent {
        ActiveEvent {
            id: ActiveId::UNSET,
            event_id: id,
            label: id.to_uppercase(),
            category: EventCategory::Car,
            origin: EventOrigin::Manual,
            started_s,
            duration_s: None,
        }
    }

    #[test]
    fn the_camera_track_follows_the_shots_without_starting_the_timeline() {
        let mut timeline = DebugTimeline::default();
        timeline.track_shot(0.0, Some("chase"));
        timeline.track_shot(3.0, Some("chase"));
        timeline.track_shot(5.0, Some("cockpit"));
        assert_eq!(timeline.current_shot(), Some("cockpit"));
        assert_eq!(timeline.shot_spans().count(), 0, "pas d'origine : rien à montrer");
        assert!(timeline.is_empty(), "la caméra seule ne remplit pas la timeline");

        let mut active = ActiveEvents::default();
        active.begin(event("a", 4.0));
        timeline.track(4.0, &active);
        let spans: Vec<_> = timeline.shot_spans().collect();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].id, "chase");
        assert!(spans[0].start_s.abs() < 1e-9, "ramené au départ de la timeline");
        assert_eq!(spans[0].end_s, Some(1.0));
        assert_eq!((spans[1].id, spans[1].end_s), ("cockpit", None));
        assert!((spans[1].start_s - 1.0).abs() < 1e-9);
        assert!(timeline.report(8.0).contains("cockpit"));
    }

    #[test]
    fn a_shot_that_ended_before_the_first_event_is_left_out() {
        let mut timeline = DebugTimeline::default();
        timeline.track_shot(0.0, Some("orbit"));
        timeline.track_shot(2.0, Some("chase"));
        let mut active = ActiveEvents::default();
        active.begin(event("a", 6.0));
        timeline.track(6.0, &active);
        let ids: Vec<_> = timeline.shot_spans().map(|s| s.id).collect();
        assert_eq!(ids, ["chase"]);
    }

    #[test]
    fn the_shot_buffer_is_capped() {
        let mut timeline = DebugTimeline::default();
        for i in 0..(MAX_SHOTS + 10) {
            timeline.track_shot(i as f64, Some(if i % 2 == 0 { "a" } else { "b" }));
        }
        assert!(timeline.shots.len() <= MAX_SHOTS);
    }

    #[test]
    fn starts_at_zero_on_the_first_event_and_closes_when_it_ends() {
        let mut timeline = DebugTimeline::default();
        let mut active = ActiveEvents::default();
        let chase = active.begin(event("chase", 10.0));
        timeline.track(10.0, &active);
        assert_eq!(timeline.spans().len(), 1);
        assert!(
            timeline.spans()[0].start_s.abs() < 1e-9,
            "la seconde zéro est la première"
        );
        assert_eq!(timeline.spans()[0].end_s, None);
        assert!(
            (timeline.length_s(14.0) - 4.0).abs() < 1e-9,
            "ouvert : jusqu'à maintenant"
        );

        active.end(chase);
        timeline.track(17.5, &active);
        assert_eq!(timeline.spans()[0].end_s, Some(7.5));
        assert!((timeline.length_s(99.0) - 7.5).abs() < 1e-9, "fini : jusqu'à sa fin");
    }

    #[test]
    fn two_overlapping_events_extend_to_the_last_end() {
        let mut timeline = DebugTimeline::default();
        let mut active = ActiveEvents::default();
        let a = active.begin(event("a", 0.0));
        timeline.track(0.0, &active);
        let b = active.begin(event("b", 2.0));
        timeline.track(2.0, &active);
        active.end(a);
        timeline.track(5.0, &active);
        active.end(b);
        timeline.track(9.0, &active);
        let ends: Vec<_> = timeline.spans().iter().map(|s| s.end_s).collect();
        assert_eq!(ends, [Some(5.0), Some(9.0)], "fins mesurées depuis la seconde 0");
        assert!((timeline.length_s(20.0) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn reset_forgets_everything_and_restarts_at_zero() {
        let mut timeline = DebugTimeline::default();
        timeline.request(3.0, "chase");
        assert!(!timeline.is_empty());
        timeline.reset();
        assert!(timeline.is_empty());
        timeline.request(50.0, "storm");
        assert_eq!(timeline.requests(), [(0.0, "storm".to_owned())]);
    }

    #[test]
    fn the_buffers_are_bounded_and_open_spans_stay_consistent() {
        let mut timeline = DebugTimeline::default();
        let mut active = ActiveEvents::default();
        let keep = active.begin(event("long", 0.0));
        timeline.track(0.0, &active);
        for i in 0..(MAX_SPANS + 10) {
            let id = active.begin(event("tick", i as f64));
            timeline.track(i as f64, &active);
            active.end(id);
            timeline.track(i as f64 + 0.5, &active);
        }
        assert_eq!(timeline.spans().len(), MAX_SPANS);
        active.end(keep);
        timeline.track(1000.0, &active);
        // Le plus ancien (l'événement long) est sorti du tampon : sa fermeture ne doit rien corrompre.
        assert!(timeline.spans().iter().all(|s| s.end_s.is_some()));
    }

    #[test]
    fn requests_are_bounded() {
        let mut timeline = DebugTimeline::default();
        for i in 0..(MAX_REQUESTS + 5) {
            timeline.request(i as f64, "chase");
        }
        assert_eq!(timeline.requests().len(), MAX_REQUESTS);
    }

    #[test]
    fn replay_emits_requests_at_their_relative_instants_then_stops() {
        let mut replay = TimelineReplay::default();
        replay.start(
            100.0,
            &[
                (0.0, "chase".to_owned()),
                (2.0, "storm".to_owned()),
                (2.0, "tower".to_owned()),
            ],
        );
        assert!(replay.running());
        let first: Vec<_> = replay.due(100.0).map(|id| id.0.to_string()).collect();
        assert_eq!(first, ["chase"]);
        assert!(replay.due(101.0).next().is_none());
        let late: Vec<_> = replay.due(102.1).map(|id| id.0.to_string()).collect();
        assert_eq!(late, ["storm", "tower"], "dans l'ordre enregistré");
        assert!(!replay.running(), "fini après la dernière");
    }

    #[test]
    fn replaying_nothing_does_not_start() {
        let mut replay = TimelineReplay::default();
        replay.start(0.0, &[]);
        assert!(!replay.running());
    }

    #[test]
    fn the_report_names_what_ran_and_when() {
        let mut timeline = DebugTimeline::default();
        let mut active = ActiveEvents::default();
        let id = active.begin(event("chase", 4.0));
        timeline.request(4.0, "chase");
        timeline.track(4.0, &active);
        active.end(id);
        timeline.track(10.0, &active);
        let report = timeline.report(10.0);
        assert!(report.contains("chase"), "{report}");
        assert!(report.contains("6.0 s"), "{report}");
        assert!(report.contains("Demandes : chase@0.0s"), "{report}");
    }
}
