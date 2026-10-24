//! Quand couper (`cinema/cutter.ts`). Hors spectacle : rythme de clip historique (coupe sur la phrase). Pendant le
//! spectacle : la grille de coupe du look (en temps musicaux, selon la section).
//!
//! La mise en scène du spectacle ([`Choreography`] : préparation et frappe du drop, salves, ordres de l'opérateur) passe
//! en tête de [`Cutter::decide`] dès qu'un spectacle tourne ; la règle générique de drop (avec sa garde de 6 s) ne
//! sert plus que hors spectacle, comme dans Sillage.

use crate::choreo::{Call, Choreography, ShowSide};
use crate::cues::CueList;
use crate::editor::{Editor, PickMode};
use crate::pacing::{CueFlags, grid_beats, max_seconds_of, min_seconds_of};
use crate::rng::CinemaRng;
use crate::shot::ShotSpec;
use crate::types::{CueKind, TransitionKind};
use crate::world::CinemaFrame;

/// Une bascule d'opportunité au plus toutes les ~22 s.
const OPPORTUNITY_GAP: f64 = 22.0;
/// Un plan neuf n'est pas coupé avant ce délai (s), drop et occasion compris.
const DROP_GUARD: f64 = 6.0;
const OPPORTUNITY_GUARD: f64 = 5.0;
/// Une animation de la voiture qui commence fait basculer sur son plan : au plus toutes les 6 s, jamais sur un plan
/// neuf.
const REACTION_GAP: f64 = 6.0;
const REACTION_GUARD: f64 = 2.5;
/// Un événement de la voiture reste valable ce temps (s) : un fondu en cours le retarde, il ne le perd pas.
const REACTION_TTL: f64 = 1.5;
/// Filet de sécurité : sans aucun temps musical, on coupe quand même ce nombre de secondes après la durée max.
const OVERRUN: f64 = 4.0;

/// Ce que le réalisateur doit lancer : un plan précis (sinon le tirage du mode), une entrée imposée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// Mode de tirage.
    pub mode: PickMode,
    /// Plan imposé (rang dans le catalogue).
    pub spec: Option<usize>,
    /// Entrée imposée.
    pub kind: Option<TransitionKind>,
    /// Le chef du spectacle a déjà compté ce plan dans la fraîcheur commune (plan de drop de `DropConductor`).
    pub counted: bool,
}

impl Decision {
    /// Tirage ordinaire dans ce mode.
    #[must_use]
    pub fn of(mode: PickMode) -> Self {
        Self {
            mode,
            spec: None,
            kind: None,
            counted: false,
        }
    }
}

/// Le plan à l'antenne, tel que le coupeur le voit.
#[derive(Debug, Clone, Copy)]
pub struct OnAir<'a> {
    /// Rang dans le catalogue.
    pub index: usize,
    /// Sa fiche.
    pub spec: &'a dyn ShotSpec,
    /// Le plan refuse d'être coupé.
    pub holding: bool,
    /// Début du plan (horloge du monde).
    pub started_at: f64,
}

/// Décide quand couper.
#[derive(Debug, Clone)]
pub struct Cutter {
    choreo: Choreography,
    flags: CueFlags,
    beat_no: u32,
    shot_beats: u32,
    reaction: bool,
    reaction_at: f64,
    last_reaction: f64,
    last_opportunity: f64,
}

impl Cutter {
    /// Coupeur dont la mise en scène tire au hasard avec `rng`.
    #[must_use]
    pub fn new(rng: CinemaRng) -> Self {
        Self {
            choreo: Choreography::new(rng),
            flags: CueFlags::default(),
            beat_no: 0,
            shot_beats: 0,
            reaction: false,
            reaction_at: f64::NEG_INFINITY,
            last_reaction: f64::NEG_INFINITY,
            last_opportunity: f64::NEG_INFINITY,
        }
    }

    /// La mise en scène (salves déjà jouées).
    #[must_use]
    pub fn choreo(&self) -> &Choreography {
        &self.choreo
    }

    /// Repart de zéro.
    pub fn reset(&mut self, show: &dyn ShowSide) {
        self.flags = CueFlags::default();
        self.beat_no = 0;
        self.shot_beats = 0;
        self.reaction = false;
        self.reaction_at = f64::NEG_INFINITY;
        self.last_reaction = f64::NEG_INFINITY;
        self.last_opportunity = f64::NEG_INFINITY;
        self.choreo.reset(show);
    }

    /// Un événement de la voiture (décollage, descente, esquive) à saisir.
    pub fn react(&mut self, time: f64) {
        self.reaction = true;
        self.reaction_at = time;
    }

    /// Un nouveau plan est à l'antenne.
    pub fn shot_started(&mut self) {
        self.shot_beats = 0;
    }

    /// Temps comptés depuis le dernier début de phrase (la grille s'y aligne).
    #[must_use]
    pub fn beat_no(&self) -> u32 {
        self.beat_no
    }

    /// Faut-il couper cette image ?
    pub fn decide(
        &mut self,
        frame: &CinemaFrame<'_>,
        cues: &CueList,
        on_air: &OnAir<'_>,
        editor: &mut Editor,
        catalog: &[Box<dyn ShotSpec>],
        show: &mut dyn ShowSide,
    ) -> Option<Decision> {
        let f = &mut self.flags;
        *f = CueFlags::default();
        for cue in cues.as_slice() {
            match cue.kind {
                CueKind::Drop => f.drop = true,
                CueKind::Phrase => {
                    f.phrase = true;
                    self.beat_no = 0;
                }
                CueKind::Bar => f.bar = true,
                CueKind::Beat => {
                    f.beat = true;
                    self.beat_no = self.beat_no.wrapping_add(1);
                    self.shot_beats = self.shot_beats.saturating_add(1);
                }
                _ => {}
            }
        }
        let f = self.flags;
        if on_air.holding {
            return None;
        }
        let view = frame.show;
        let elapsed = frame.time - on_air.started_at;
        if view.active {
            match self
                .choreo
                .decide(frame, f, elapsed, Some(on_air.index), editor, catalog, show)
            {
                Some(Call::Wait) => return None,
                Some(Call::Decide(decision)) => return Some(decision),
                None if view.drop_now => return None,
                None => {}
            }
        }
        let show: &dyn ShowSide = show;
        let breathing = view.active && view.breathing;
        let min_seconds = f64::from(min_seconds_of(frame, on_air.spec));
        let max_seconds = f64::from(max_seconds_of(frame, on_air.spec));
        if f.drop && !breathing && elapsed > DROP_GUARD {
            return Some(Decision::of(PickMode::Drop));
        }
        if self.reaction && frame.time - self.reaction_at > REACTION_TTL {
            self.reaction = false;
        }
        if self.reaction
            && elapsed > REACTION_GUARD
            && frame.time - self.last_reaction >= REACTION_GAP
            && (!on_air.spec.scripted() || elapsed >= f64::from(on_air.spec.min_seconds()))
            && let Some(reply) = editor.best_opportunity(frame, show, catalog, Some(on_air.index), true)
        {
            self.last_reaction = frame.time;
            self.reaction = false;
            return Some(Decision {
                spec: Some(reply),
                ..Decision::of(PickMode::Normal)
            });
        }
        if f.beat
            && elapsed > OPPORTUNITY_GUARD
            && frame.time - self.last_opportunity >= OPPORTUNITY_GAP
            && let Some(chance) = editor.best_opportunity(frame, show, catalog, Some(on_air.index), false)
        {
            self.last_opportunity = frame.time;
            return Some(Decision {
                spec: Some(chance),
                ..Decision::of(PickMode::Normal)
            });
        }
        if view.active {
            let grid = grid_beats(frame);
            if f.beat && self.beat_no.is_multiple_of(grid) && self.shot_beats >= grid && elapsed >= min_seconds {
                return Some(Decision::of(PickMode::Normal));
            }
        } else if f.phrase && elapsed >= min_seconds {
            return Some(Decision::of(PickMode::Normal));
        }
        if f.bar && elapsed >= max_seconds {
            return Some(Decision::of(PickMode::Normal));
        }
        (elapsed >= max_seconds + OVERRUN).then_some(Decision::of(PickMode::Normal))
    }
}
