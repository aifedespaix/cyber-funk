//! Quand couper (`cinema/cutter.ts`). Hors spectacle : rythme de clip historique (coupe sur la phrase). Pendant le
//! spectacle : la grille de coupe du look (en temps musicaux, selon la section).
//!
//! La mise en scène du spectacle (`Choreography` : préparation et frappe du drop, salves, ordres de l'opérateur) se
//! branche en tête de [`Cutter::decide`] avec J6-02.

use crate::cues::CueList;
use crate::editor::{Editor, PickMode};
use crate::pacing::{CueFlags, grid_beats, max_seconds_of, min_seconds_of};
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
}

impl Decision {
    /// Tirage ordinaire dans ce mode.
    #[must_use]
    pub fn of(mode: PickMode) -> Self {
        Self {
            mode,
            spec: None,
            kind: None,
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
#[derive(Debug, Clone, Default)]
pub struct Cutter {
    flags: CueFlags,
    beat_no: u32,
    shot_beats: u32,
    reaction: bool,
    reaction_at: f64,
    last_reaction: f64,
    last_opportunity: f64,
}

impl Cutter {
    /// Repart de zéro.
    pub fn reset(&mut self) {
        *self = Self {
            reaction_at: f64::NEG_INFINITY,
            last_reaction: f64::NEG_INFINITY,
            last_opportunity: f64::NEG_INFINITY,
            ..Self::default()
        };
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
        editor: &Editor,
        catalog: &[Box<dyn ShotSpec>],
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
        let show = frame.show;
        let elapsed = frame.time - on_air.started_at;
        let breathing = show.active && show.breathing;
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
            && let Some(reply) = editor.best_opportunity(frame, catalog, Some(on_air.index), true)
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
            && let Some(chance) = editor.best_opportunity(frame, catalog, Some(on_air.index), false)
        {
            self.last_opportunity = frame.time;
            return Some(Decision {
                spec: Some(chance),
                ..Decision::of(PickMode::Normal)
            });
        }
        if show.active {
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
