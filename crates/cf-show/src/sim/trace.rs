//! La trace d'une simulation : une ligne par fait marquant (cue, appel, passation, drop, bascule, portes).

use std::fmt::Write as _;

use crate::director::{ShowDirector, ShowEvent, ShowState};
use crate::model::{Event, ShowGates};

/// Heure de set lisible : `H:MM:SS.d` (dixièmes de seconde).
#[must_use]
pub fn format_time(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    let (h, m, s) = (tenths / 36_000, tenths / 600 % 60, tenths / 10 % 60);
    format!("{h}:{m:02}:{s:02}.{}", tenths % 10)
}

/// Les portes qui changent les décisions du monde (booléens, paliers, listes) ; les nombres qui glissent sont
/// laissés de côté : une trace n'a pas à porter chaque pas d'interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "copie des booléens indépendants de `ShowGates`, comparée d'une image à l'autre"
)]
struct GateKey {
    max_floor: u8,
    flight_on_drop_only: bool,
    tricks: bool,
    salvo: bool,
    meteor_shower: bool,
    ascension: bool,
    stunt_tiers: usize,
    shot_tiers: usize,
    events: usize,
}

impl GateKey {
    fn of(g: &ShowGates) -> Self {
        Self {
            max_floor: g.max_floor,
            flight_on_drop_only: g.flight_on_drop_only,
            tricks: g.tricks,
            salvo: g.salvo,
            meteor_shower: g.meteor_shower,
            ascension: g.ascension,
            stunt_tiers: g.stunt_tiers.len(),
            shot_tiers: g.shot_tiers.len(),
            events: g.events.len(),
        }
    }

    fn describe(self) -> String {
        let on = |b: bool| if b { "oui" } else { "non" };
        format!(
            "étage max {} · vol au drop seul {} · figures {} · salve {} · pluie de météores {} · ascension {} · \
             paliers de cascades {} · paliers de plans {} · événements {}",
            self.max_floor,
            on(self.flight_on_drop_only),
            on(self.tricks),
            on(self.salvo),
            on(self.meteor_shower),
            on(self.ascension),
            self.stunt_tiers,
            self.shot_tiers,
            self.events,
        )
    }
}

/// Ce qu'une simulation a compté.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Images simulées.
    pub frames: u64,
    /// Cues entrés (ouverture comprise).
    pub cues_entered: u32,
    /// Drops qui ont frappé.
    pub drops: u32,
    /// Cartons-titres publiés.
    pub titles: u32,
    /// Passations de DJ commencées.
    pub handovers: u32,
    /// Plus grand nombre d'événements publiés en une seule image (la file du chef du spectacle en tient 64).
    pub max_events_per_frame: usize,
}

/// La trace complète d'une simulation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trace {
    lines: Vec<String>,
    /// Compteurs.
    pub stats: Stats,
}

impl Trace {
    /// Les lignes, dans l'ordre du set.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Le texte de la trace, une ligne par fait, suivi du résumé : de quoi comparer par instantané ou lire.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = self.lines.join("\n");
        let s = &self.stats;
        let _ = write!(
            out,
            "\n---\n{} cues, {} drops, {} titres, {} passations ; pic de {} événements par image",
            s.cues_entered, s.drops, s.titles, s.handovers, s.max_events_per_frame
        );
        out
    }
}

/// Observe le chef du spectacle image par image et écrit la trace. Ne formate que sur un changement : le coût par
/// image reste celui de quelques comparaisons.
#[derive(Debug)]
pub struct Recorder {
    slot_ids: Vec<String>,
    cue_ids: Vec<Vec<String>>,
    trace: Trace,
    gates: Option<GateKey>,
}

impl Recorder {
    /// Prépare l'enregistrement d'un événement (les noms des créneaux et des cues viennent de lui).
    #[must_use]
    pub fn new(event: &Event) -> Self {
        Self {
            slot_ids: event.slots.iter().map(|s| s.id.to_string()).collect(),
            cue_ids: event
                .slots
                .iter()
                .map(|s| s.cues.iter().map(|c| c.id.to_string()).collect())
                .collect(),
            trace: Trace::default(),
            gates: None,
        }
    }

    fn slot(&self, slot: usize) -> &str {
        self.slot_ids.get(slot).map_or("?", String::as_str)
    }

    fn cue(&self, slot: usize, cue: usize) -> String {
        let id = self
            .cue_ids
            .get(slot)
            .and_then(|c| c.get(cue))
            .map_or("?", String::as_str);
        format!("{}/{id}", self.slot(slot))
    }

    fn log(&mut self, t: f64, text: &str) {
        self.trace.lines.push(format!("{}  {text}", format_time(t)));
    }

    /// Note ce que l'image `t` (s depuis le début du set) a produit : `events` vidés juste après `update`.
    pub fn observe<R: rand_core::Rng>(&mut self, t: f64, director: &ShowDirector<R>, events: &[ShowEvent]) {
        let stats = &mut self.trace.stats;
        stats.frames += 1;
        stats.max_events_per_frame = stats.max_events_per_frame.max(events.len());
        for event in events {
            self.event(t, event);
        }
        let state: &ShowState = director.state();
        if state.drop_now {
            self.trace.stats.drops += 1;
            let n = self.trace.stats.drops;
            let text = match director.drop_plan() {
                Some(plan) => format!(
                    "DROP n°{n} · cascade {} · plan {} · flash {:.2}{}",
                    plan.stunt.as_ref().map_or_else(|| "-".to_owned(), ToString::to_string),
                    plan.shot.as_ref().map_or_else(|| "-".to_owned(), ToString::to_string),
                    plan.fx.flash,
                    if plan.forced { " · forcé" } else { "" },
                ),
                None => format!("DROP n°{n}"),
            };
            self.log(t, &text);
        }
        let key = GateKey::of(state.gates());
        if self.gates != Some(key) {
            self.gates = Some(key);
            self.log(t, &format!("portes : {}", key.describe()));
        }
    }

    fn event(&mut self, t: f64, event: &ShowEvent) {
        let text = match event {
            ShowEvent::CueCalled {
                slot,
                cue,
                entry,
                cause,
                ..
            } => format!("appel {} ({cause:?}, entrée {entry:?})", self.cue(*slot, *cue)),
            ShowEvent::CueEntered { slot, cue, cause, .. } => {
                self.trace.stats.cues_entered += 1;
                format!("cue {} ({cause:?})", self.cue(*slot, *cue))
            }
            ShowEvent::Preload { slot } => format!("préparation du créneau {}", self.slot(*slot)),
            ShowEvent::Handover { from, to, fade_s } => {
                self.trace.stats.handovers += 1;
                format!(
                    "passation {} -> {} (fondu {fade_s:.1} s)",
                    self.slot(*from),
                    self.slot(*to)
                )
            }
            ShowEvent::SlotLoaded { slot, previous } => {
                format!("créneau chargé {} (après {})", self.slot(*slot), self.slot(*previous))
            }
            ShowEvent::ShowDj { slot } => format!("rappel du DJ {}", self.slot(*slot)),
            ShowEvent::DjLogo { slot } => format!("logo du DJ {}", self.slot(*slot)),
            ShowEvent::Title(title) => {
                self.trace.stats.titles += 1;
                format!("titre « {title} »")
            }
            ShowEvent::Toggle { what, on } => format!("bascule {what:?} : {}", if *on { "oui" } else { "non" }),
        };
        self.log(t, &text);
    }

    /// Rend la trace.
    #[must_use]
    pub fn finish(self) -> Trace {
        self.trace
    }
}
