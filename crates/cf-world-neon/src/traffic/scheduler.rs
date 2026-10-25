//! Le tirage des événements (`schedule`, `choose`, `showAllows`, `rate` de `director.ts`) : quand lancer, lequel, dans le
//! respect des portes du look. Pur et déterministe (hasard injecté) ; le directeur l'appelle une fois par image.
//!
//! Le délai se raccourcit quand la musique est intense, un événement attend le prochain temps fort pour démarrer, on ne
//! répète jamais les deux derniers tirés, et les événements peuvent se chevaucher (deux à la fois, jamais deux
//! structures).

use super::air_lanes::range;
use super::event::EventRegistry;
use super::frame::TrafficFrame;
use super::names::{EVENT_COUNT, EventName, MAX_PARALLEL, TITLE_WINDOW_S};
use crate::pilot::rand::Rand;

/// Mémoire des derniers événements tirés (`history`) : on n'en répète aucun des deux derniers.
const HISTORY: usize = 6;
/// Un événement en attente part au plus tard après ce temps (s), même sans début de mesure.
const PENDING_MAX_S: f32 = 4.0;

/// Ce que le trafic demande au chef du spectacle : l'ordre « événement au hasard » de l'opérateur et la mémoire
/// d'anti-répétition (`freshness`). Sans spectacle : [`NoShow`].
pub trait ShowLink {
    /// Vrai une seule fois après un ordre « événement au hasard » encore vivant (`take("randomEvent")`).
    fn take_random_event(&mut self) -> bool;
    /// Coefficient à multiplier au poids de tirage de `id` (`event:<slug>`), 1 pour un élément frais.
    fn freshness_weight(&self, id: &str, now: f64) -> f64;
    /// Marque `id` comme utilisé à l'instant `now`.
    fn freshness_mark(&mut self, id: &str, now: f64);
}

/// Pas de spectacle : aucun ordre, aucune mémoire.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoShow;

impl ShowLink for NoShow {
    fn take_random_event(&mut self) -> bool {
        false
    }

    fn freshness_weight(&self, _id: &str, _now: f64) -> f64 {
        1.0
    }

    fn freshness_mark(&mut self, _id: &str, _now: f64) {}
}

/// Un événement en cours, vu du tirage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Running {
    /// Lequel.
    pub name: EventName,
    /// Occupe la route (ne se chevauche pas).
    pub structure: bool,
}

/// L'état du tirage.
#[derive(Debug, Clone)]
pub struct Scheduler {
    countdown: f32,
    pending: Option<EventName>,
    pending_since: f32,
    history: Vec<EventName>,
    /// Compteurs de DJ et de niveau vus, et l'instant du dernier changement (pour le titre holographique).
    seen_dj: u32,
    seen_level: u32,
    title_since: f64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            countdown: 8.0,
            pending: None,
            pending_since: 0.0,
            history: Vec::with_capacity(HISTORY + 1),
            seen_dj: 0,
            seen_level: 0,
            title_since: f64::NEG_INFINITY,
        }
    }
}

impl Scheduler {
    /// Secondes avant le prochain tirage (outil de développement et tests).
    #[must_use]
    pub fn countdown(&self) -> f32 {
        self.countdown
    }

    /// L'événement tiré qui attend le début d'une mesure.
    #[must_use]
    pub fn pending(&self) -> Option<EventName> {
        self.pending
    }

    /// Instant du dernier changement de DJ ou de niveau.
    #[must_use]
    pub fn title_since(&self) -> f64 {
        self.title_since
    }

    /// Repère les changements de DJ et de niveau : le titre holographique est alors tiré en priorité, assez vite
    /// (`trackShow`).
    pub fn track_show(&mut self, frame: &TrafficFrame<'_>, rng: &mut Rand) {
        let Some(show) = frame.show else { return };
        if show.dj_seq == self.seen_dj && show.level_seq == self.seen_level {
            return;
        }
        self.seen_dj = show.dj_seq;
        self.seen_level = show.level_seq;
        self.title_since = frame.time;
        if self.countdown > 7.0 {
            self.countdown = range(rng, 4.0, 7.0);
        }
    }

    /// Cadence des événements : les portes du niveau, au plus lente en respiration. Hors cinéma : 1 (`rate`).
    #[must_use]
    pub fn rate(frame: &TrafficFrame<'_>) -> f32 {
        match frame.show {
            None => 1.0,
            Some(show) if show.breathing => show.event_rate.min(0.5) * 0.6,
            Some(show) => show.event_rate,
        }
    }

    /// Les portes du spectacle laissent-elles tirer cet événement ? Hors cinéma : tout sauf les événements du
    /// spectacle (`showAllows`).
    #[must_use]
    pub fn allows(name: EventName, frame: &TrafficFrame<'_>) -> bool {
        let Some(show) = frame.show else {
            return !name.is_show_only();
        };
        if !show.allows(name) || (show.breathing && !name.is_breathing_ok()) {
            return false;
        }
        match name {
            EventName::CityPulse => show.city_pulse >= 0.3,
            EventName::HoloTitle => show.titles,
            _ => true,
        }
    }

    /// Tire l'événement à lancer maintenant s'il y en a un, après l'ordre de l'opérateur, l'attente d'un début de
    /// mesure ou le délai écoulé. `bar_started` : une mesure vient de commencer. Renvoie l'événement à démarrer.
    pub fn next(
        &mut self,
        frame: &TrafficFrame<'_>,
        running: &[Running],
        registry: &EventRegistry,
        link: &mut dyn ShowLink,
        bar_started: bool,
        rng: &mut Rand,
    ) -> Option<EventName> {
        // Événement demandé par l'opérateur : tout de suite, sans compte à rebours (les règles de structures et de
        // ciel restent).
        if frame.show.is_some()
            && link.take_random_event()
            && let Some(name) = self.choose(frame, running, registry, link, rng)
        {
            self.pending = None;
            return Some(name);
        }
        // Un événement en attente que les portes n'autorisent plus (changement de niveau, respiration) est abandonné.
        if self.pending.is_some_and(|name| !Self::allows(name, frame)) {
            self.pending = None;
        }
        if let Some(name) = self.pending {
            // On attend le début d'une mesure (horloge de danse, calée sur le tempo) ; au bout de quatre secondes,
            // tant pis.
            self.pending_since += frame.dt;
            return (bar_started || self.pending_since > PENDING_MAX_S).then_some(name);
        }
        let energy = (frame.beat.bass + frame.beat.mid).min(1.0);
        self.countdown -= frame.dt * (0.75 + energy * 0.7) * Self::rate(frame);
        if self.countdown > 0.0 {
            return None;
        }
        match self.choose(frame, running, registry, link, rng) {
            Some(name) => {
                self.pending = Some(name);
                self.pending_since = 0.0;
            }
            None => self.countdown = 3.0,
        }
        None
    }

    /// L'événement `name` démarre : on retient son tirage et le délai avant le suivant repart (`start`).
    pub fn started(&mut self, name: EventName, frame: &TrafficFrame<'_>, link: &mut dyn ShowLink, rng: &mut Rand) {
        self.pending = None;
        if frame.show.is_some() {
            link.freshness_mark(name.freshness_id(), frame.time);
        }
        if self.history.len() >= HISTORY {
            self.history.remove(0);
        }
        self.history.push(name);
        self.countdown = range(rng, 15.0, 34.0);
    }

    /// Le démarrage est reporté (une structure ne se pose pas sur une arche-immeuble) : on retente plus tard.
    pub fn postponed(&mut self) {
        self.pending = None;
        self.countdown = 4.0;
    }

    /// Ce que la situation écarte : les portes, le mode calme, la répétition, les voisins en cours et la hauteur de la
    /// voiture (`high` : sa plus haute altitude, actuelle ou visée).
    fn excluded(
        name: EventName,
        frame: &TrafficFrame<'_>,
        running: &[Running],
        recent: &[EventName],
        high: f32,
    ) -> bool {
        let with = |other: EventName| running.iter().any(|r| r.name == other);
        if !Self::allows(name, frame) || recent.contains(&name) || with(name) {
            return true;
        }
        if frame.calm && !name.is_calm_ok() {
            return true;
        }
        // La voiture vole : l'escorte (au ras de la route), la nuée et le vaisseau publicitaire occupent des hauteurs
        // qui sont les siennes.
        if name.max_car_altitude().is_some_and(|max| high > max) {
            return true;
        }
        // Une structure ne se pose pas sur une autre ; un seul événement de ciel à la fois.
        if name.is_structure_kind() && running.iter().any(|r| r.structure) {
            return true;
        }
        if name.is_sky() && running.iter().any(|r| r.name.is_sky()) {
            return true;
        }
        // Le titre holographique et l'aurore se disputent le haut du ciel.
        (name == EventName::HoloTitle && with(EventName::Aurora))
            || (name == EventName::Aurora && with(EventName::HoloTitle))
    }

    /// Tire un événement parmi ceux que la situation permet, au poids de tirage (`choose`).
    #[must_use]
    pub fn choose(
        &self,
        frame: &TrafficFrame<'_>,
        running: &[Running],
        registry: &EventRegistry,
        link: &dyn ShowLink,
        rng: &mut Rand,
    ) -> Option<EventName> {
        if running.len() >= MAX_PARALLEL {
            return None;
        }
        let recent = &self.history[self.history.len().saturating_sub(2)..];
        let high = frame.altitude.max(frame.target);
        let mut weights = [0.0_f64; EVENT_COUNT];
        let mut total = 0.0;
        for name in EventName::ALL {
            let allowed = registry.contains(name) && !Self::excluded(name, frame, running, recent, high);
            if !allowed {
                continue;
            }
            let mut weight = name.weight() * name.section_bias(frame.drive.section);
            if frame.show.is_some() {
                weight *= link.freshness_weight(name.freshness_id(), frame.time);
                // Le titre du DJ passe de préférence peu après un changement de DJ ou de niveau.
                if name == EventName::HoloTitle {
                    weight *= if frame.time - self.title_since < TITLE_WINDOW_S {
                        6.0
                    } else {
                        0.4
                    };
                }
            }
            weights[name.index()] = weight;
            total += weight;
        }
        if total <= 0.0 {
            return None;
        }
        let mut roll = rng.unit() * total;
        let mut last = None;
        for name in EventName::ALL {
            let weight = weights[name.index()];
            if weight <= 0.0 {
                continue;
            }
            last = Some(name);
            roll -= weight;
            if roll <= 0.0 {
                return Some(name);
            }
        }
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::event::{EventCtx, TrafficEvent};
    use crate::traffic::frame::TrafficShow;
    use cf_show::model::EventName as ShowEventName;

    #[derive(Debug)]
    struct Nop;
    impl TrafficEvent for Nop {
        fn update(&mut self, _dt: f32, _ctx: &mut EventCtx<'_>) -> bool {
            false
        }

        fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
    }

    fn registry_of(names: &[EventName]) -> EventRegistry {
        let mut registry = EventRegistry::default();
        for &name in names {
            registry.register(name, Box::new(|_| Box::new(Nop)));
        }
        registry
    }

    fn show(events: &[ShowEventName]) -> TrafficShow<'_> {
        TrafficShow {
            level: 3,
            breathing: false,
            flashes: true,
            ground_traffic: 0.5,
            event_rate: 1.0,
            events,
            city_pulse: 0.6,
            titles: true,
            dj_seq: 0,
            level_seq: 0,
        }
    }

    fn slugs(names: &[EventName]) -> Vec<ShowEventName> {
        names
            .iter()
            .map(|n| ShowEventName::new(n.as_str()).expect("slug"))
            .collect()
    }

    #[test]
    fn hors_spectacle_les_evenements_du_spectacle_ne_sont_jamais_tires() {
        let frame = TrafficFrame::default();
        assert!(Scheduler::allows(EventName::Chase, &frame));
        assert!(!Scheduler::allows(EventName::Tunnel, &frame));
        assert!(!Scheduler::allows(EventName::CityPulse, &frame));
        assert!(!Scheduler::allows(EventName::HoloTitle, &frame));
    }

    #[test]
    fn les_portes_du_look_font_la_liste_blanche() {
        let events = slugs(&[EventName::Chase, EventName::Fish, EventName::CityPulse]);
        let mut s = show(&events);
        let frame = TrafficFrame {
            show: Some(s),
            ..TrafficFrame::default()
        };
        assert!(Scheduler::allows(EventName::Chase, &frame));
        assert!(!Scheduler::allows(EventName::Storm, &frame));
        // La pulsation de la ville a sa propre porte.
        s.city_pulse = 0.2;
        let frame = TrafficFrame {
            show: Some(s),
            ..TrafficFrame::default()
        };
        assert!(!Scheduler::allows(EventName::CityPulse, &frame));
        // La respiration ne garde que le doux.
        s.city_pulse = 0.6;
        s.breathing = true;
        let frame = TrafficFrame {
            show: Some(s),
            ..TrafficFrame::default()
        };
        assert!(!Scheduler::allows(EventName::Chase, &frame));
        assert!(Scheduler::allows(EventName::Fish, &frame));
        assert!((Scheduler::rate(&frame) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn rien_n_est_tire_sans_evenement_inscrit() {
        let scheduler = Scheduler::default();
        let mut rng = Rand::new(1);
        let frame = TrafficFrame::default();
        assert_eq!(
            scheduler.choose(&frame, &[], &EventRegistry::default(), &NoShow, &mut rng),
            None
        );
    }

    #[test]
    fn jamais_trois_a_la_fois_ni_le_meme_deux_fois_ni_deux_structures() {
        let scheduler = Scheduler::default();
        let registry = registry_of(&[
            EventName::Chase,
            EventName::Tower,
            EventName::Crossroad,
            EventName::Fish,
        ]);
        let mut rng = Rand::new(2);
        let frame = TrafficFrame::default();
        let full = [
            Running {
                name: EventName::Chase,
                structure: false,
            },
            Running {
                name: EventName::Fish,
                structure: false,
            },
        ];
        assert_eq!(scheduler.choose(&frame, &full, &registry, &NoShow, &mut rng), None);
        let one = [Running {
            name: EventName::Tower,
            structure: true,
        }];
        for _ in 0..200 {
            let pick = scheduler
                .choose(&frame, &one, &registry, &NoShow, &mut rng)
                .expect("un choix");
            assert!(!pick.is_structure_kind(), "{pick:?} avec une structure en cours");
            assert_ne!(pick, EventName::Tower);
        }
    }

    #[test]
    fn la_voiture_qui_vole_ecarte_les_evenements_qui_prennent_sa_hauteur() {
        let scheduler = Scheduler::default();
        let registry = registry_of(&[EventName::Escort, EventName::Race, EventName::Swarm, EventName::Fish]);
        let mut rng = Rand::new(3);
        let frame = TrafficFrame {
            altitude: 21.0,
            target: 21.0,
            ..TrafficFrame::default()
        };
        for _ in 0..200 {
            assert_eq!(
                scheduler.choose(&frame, &[], &registry, &NoShow, &mut rng),
                Some(EventName::Fish)
            );
        }
    }

    #[test]
    fn un_evenement_attend_le_debut_de_mesure_puis_part_et_le_delai_repart() {
        let mut scheduler = Scheduler::default();
        let registry = registry_of(&[EventName::Fish]);
        let mut rng = Rand::new(4);
        let mut link = NoShow;
        let frame = TrafficFrame {
            dt: 1.0,
            ..TrafficFrame::default()
        };
        // 8 s de délai, à une vitesse de ~0,75 : tiré au bout de 11 images d'une seconde.
        let mut started = None;
        for i in 0..40 {
            if let Some(name) = scheduler.next(&frame, &[], &registry, &mut link, false, &mut rng) {
                started = Some((i, name));
                break;
            }
        }
        let (i, name) = started.expect("parti sans début de mesure au bout de 4 s");
        assert_eq!(name, EventName::Fish);
        assert!(i >= 14, "il a attendu ses 4 s d'attente : image {i}");
        scheduler.started(name, &frame, &mut link, &mut rng);
        assert!((15.0..34.0).contains(&scheduler.countdown()));
        assert_eq!(scheduler.pending(), None);
    }

    #[test]
    fn un_debut_de_mesure_declenche_tout_de_suite() {
        let mut scheduler = Scheduler::default();
        let registry = registry_of(&[EventName::Fish]);
        let mut rng = Rand::new(5);
        let frame = TrafficFrame {
            dt: 1.0,
            ..TrafficFrame::default()
        };
        for _ in 0..12 {
            scheduler.next(&frame, &[], &registry, &mut NoShow, false, &mut rng);
        }
        assert_eq!(scheduler.pending(), Some(EventName::Fish));
        assert_eq!(
            scheduler.next(&frame, &[], &registry, &mut NoShow, true, &mut rng),
            Some(EventName::Fish)
        );
    }

    #[test]
    fn un_changement_de_dj_raccourcit_l_attente_et_favorise_le_titre() {
        let mut scheduler = Scheduler::default();
        let mut rng = Rand::new(6);
        let events = slugs(&[EventName::HoloTitle, EventName::Fish]);
        let mut s = show(&events);
        s.dj_seq = 1;
        let frame = TrafficFrame {
            show: Some(s),
            time: 100.0,
            ..TrafficFrame::default()
        };
        scheduler.track_show(&frame, &mut rng);
        assert!(scheduler.countdown() <= 7.0);
        assert!((scheduler.title_since() - 100.0).abs() < f64::EPSILON);
        let registry = registry_of(&[EventName::HoloTitle, EventName::Fish]);
        let titles = (0..1000)
            .filter(|_| scheduler.choose(&frame, &[], &registry, &NoShow, &mut rng) == Some(EventName::HoloTitle))
            .count();
        assert!(titles > 650, "{titles} titres sur 1000");
    }

    #[test]
    fn l_ordre_de_l_operateur_lance_sans_attendre() {
        struct Operator(bool);
        impl ShowLink for Operator {
            fn take_random_event(&mut self) -> bool {
                std::mem::take(&mut self.0)
            }

            fn freshness_weight(&self, _id: &str, _now: f64) -> f64 {
                1.0
            }

            fn freshness_mark(&mut self, _id: &str, _now: f64) {}
        }
        let mut scheduler = Scheduler::default();
        let registry = registry_of(&[EventName::Fish]);
        let events = slugs(&[EventName::Fish]);
        let frame = TrafficFrame {
            show: Some(show(&events)),
            ..TrafficFrame::default()
        };
        let mut rng = Rand::new(7);
        let mut link = Operator(true);
        assert_eq!(
            scheduler.next(&frame, &[], &registry, &mut link, false, &mut rng),
            Some(EventName::Fish)
        );
        assert_eq!(scheduler.next(&frame, &[], &registry, &mut link, false, &mut rng), None);
    }
}
