//! La mise en scène du spectacle (`cinema/choreo.ts`) : préparation et frappe du drop, salves de coupes, ordres de
//! l'opérateur (plan suivant, enseigne du DJ). Elle ne tire jamais au hasard hors de son propre générateur et passe par
//! l'[`Editor`] pour tout choix de plan.
//!
//! # Sécurité du public
//!
//! - Aucun plan n'est lancé sans passer par `Editor::can_use`, qui écarte `strobe` quand les flashs sont coupés ou en
//!   respiration (le plan de drop préparé par `DropConductor` peut être `strobe` : il est alors refusé, le tirage
//!   ordinaire prend le relais).
//! - Les salves (3 ou 4 coupes d'une mesure) sont aussi plafonnées par [`MAX_CUTS_PER_SECOND`], appliqué par le
//!   réalisateur sur toutes les coupes. À 174 BPM une mesure dure 1,38 s : une salve ne dépasse jamais 3 coupes par
//!   seconde, une coupe de plus est retardée.

mod cadence;
mod drop;
mod side;
#[cfg(test)]
mod tests;

pub use cadence::{CutLimiter, MAX_CUTS_PER_SECOND, WINDOW_SECONDS, is_jump};
pub use side::{NoShow, ShowSide};

use cf_show::director::Order;

use crate::cutter::Decision;
use crate::editor::{Editor, PickMode};
use crate::pacing::CueFlags;
use crate::rng::CinemaRng;
use crate::shot::ShotSpec;
use crate::world::CinemaFrame;

/// Plans de préparation d'un drop : ils poussent (travelling avant, grue qui monte, focale qui se resserre).
const PUSH_IDS: &[&str] = &["push-in", "crane", "dolly-zoom", "vertigo"];
const PUSH_NO_DOLLY: &[&str] = &["push-in", "crane", "vertigo"];
/// Un plan neuf n'est pas coupé par le drop avant ce délai (s).
pub const HIT_GUARD: f64 = 1.2;
/// Un plan neuf n'est pas remplacé par un plan de préparation avant ce délai (s).
pub const PREP_GUARD: f64 = 2.5;
/// Un plan de préparation n'est jamais coupé par la routine avant ce délai (s) : le drop viendra.
pub const PREP_HOLD: f64 = 24.0;
/// Une cascade dont le plan n'est pas encore jouable le reste ce temps (s).
const PENDING_SECONDS: f64 = 2.5;
/// Un ordre « plan suivant » attend ce temps (s) un temps musical pour couper.
const SKIP_TTL: f64 = 2.5;
/// L'enseigne du DJ attend ce temps (s) un temps musical après un changement de créneau, 8 s après un ordre.
const DJ_TTL: f64 = 20.0;
const DJ_ORDER_TTL: f64 = 8.0;
/// Chance d'un bullet-time sur un drop sans plan prévu.
const BULLET_CHANCE: f32 = 0.35;
/// Salve : au plus une toutes les 3 à 4 minutes.
pub const SALVO_GAP: f64 = 180.0;
/// Écart supplémentaire tiré au hasard (s) entre deux salves.
pub const SALVO_GAP_SPAN: f64 = 60.0;
/// Une coupe de salve attend ce temps (s) depuis la précédente.
const SALVO_MIN_CUT: f64 = 0.9;
/// Salves gardées en mémoire (de quoi vérifier la rareté, sans grossir pendant six heures de show).
const SALVO_HISTORY: usize = 64;

/// Ce que la mise en scène répond au coupeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    /// Rien à faire, et la routine ordinaire doit se taire.
    Wait,
    /// Lancer ce plan.
    Decide(Decision),
}

/// La mise en scène du spectacle.
#[derive(Debug, Clone)]
pub struct Choreography {
    /// Instants (horloge du monde) où une salve a commencé, les plus récents : de quoi vérifier la rareté.
    pub salvo_at: Vec<f64>,
    prepared: bool,
    pending: Option<usize>,
    pending_until: f64,
    salvo_left: u32,
    salvo_until: f64,
    last_salvo: f64,
    salvo_gap: f64,
    dj_seq: u32,
    dj_until: f64,
    dj_forced: bool,
    skip_until: f64,
    rng: CinemaRng,
}

impl Choreography {
    /// Mise en scène de graine `seed`.
    #[must_use]
    pub fn new(rng: CinemaRng) -> Self {
        Self {
            salvo_at: Vec::with_capacity(SALVO_HISTORY),
            prepared: false,
            pending: None,
            pending_until: 0.0,
            salvo_left: 0,
            salvo_until: 0.0,
            last_salvo: f64::NEG_INFINITY,
            salvo_gap: SALVO_GAP,
            dj_seq: 0,
            dj_until: f64::NEG_INFINITY,
            dj_forced: false,
            skip_until: f64::NEG_INFINITY,
            rng,
        }
    }

    /// Démarrage du réalisateur.
    pub fn reset(&mut self, show: &dyn ShowSide) {
        self.prepared = false;
        self.pending = None;
        self.salvo_left = 0;
        self.last_salvo = f64::NEG_INFINITY;
        self.dj_seq = show.slot_seq();
        self.dj_until = f64::NEG_INFINITY;
        self.skip_until = f64::NEG_INFINITY;
        self.salvo_at.clear();
    }

    /// Une salve est en cours : la routine de coupe se tait.
    #[must_use]
    pub fn in_salvo(&self) -> bool {
        self.salvo_left > 0
    }

    /// `Some(Call::Wait)` : rien à faire et la routine ordinaire doit se taire ; `None` : rien de spécial.
    #[allow(clippy::too_many_arguments)] // Le contexte d'une décision : image, repères, plan à l'antenne, catalogue.
    pub fn decide(
        &mut self,
        frame: &CinemaFrame<'_>,
        flags: CueFlags,
        elapsed: f64,
        spec: Option<usize>,
        editor: &mut Editor,
        catalog: &[Box<dyn ShotSpec>],
        show: &mut dyn ShowSide,
    ) -> Option<Call> {
        let now = frame.time;
        if show.take(Order::SkipShot) {
            self.skip_until = now + SKIP_TTL;
        }
        if show.take(Order::ShowDj) {
            self.dj_until = now + DJ_ORDER_TTL;
            self.dj_forced = true;
        }
        if show.slot_seq() != self.dj_seq {
            self.dj_seq = show.slot_seq();
            self.dj_until = now + DJ_TTL;
            self.dj_forced = false;
        }
        let show: &dyn ShowSide = show;
        let breathing = frame.show.breathing;

        if frame.show.drop_now {
            return self
                .on_drop(frame, elapsed, spec, editor, catalog, show)
                .map(Call::Decide);
        }
        if let Some(index) = self.pending {
            if now > self.pending_until || breathing {
                self.pending = None;
            } else if spec != Some(index) && editor.can_use(frame, show, catalog, index, false) {
                self.pending = None;
                return Some(Call::Decide(Decision {
                    spec: Some(index),
                    kind: Some(crate::types::TransitionKind::Cut),
                    ..Decision::of(PickMode::Drop)
                }));
            }
        }
        if self.salvo_left > 0 {
            if now > self.salvo_until || breathing {
                self.salvo_left = 0;
            } else if flags.bar && elapsed >= SALVO_MIN_CUT {
                self.salvo_left -= 1;
                return Some(Call::Decide(Decision {
                    kind: Some(crate::types::TransitionKind::Cut),
                    ..Decision::of(PickMode::Drop)
                }));
            } else {
                return Some(Call::Wait);
            }
        }
        if self.dj_until > now
            && let Some(sign) = editor.find("dj-sign")
        {
            if spec == Some(sign) {
                self.dj_until = f64::NEG_INFINITY;
            } else if flags.beat && elapsed > 1.2 && editor.can_use(frame, show, catalog, sign, self.dj_forced) {
                self.dj_until = f64::NEG_INFINITY;
                return Some(Call::Decide(Decision {
                    spec: Some(sign),
                    ..Decision::of(PickMode::Normal)
                }));
            }
        }
        if self.skip_until > now && flags.beat {
            self.skip_until = f64::NEG_INFINITY;
            return Some(Call::Decide(Decision::of(PickMode::Normal)));
        }
        self.prepare(frame, flags, elapsed, spec, editor, catalog, show)
    }

    /// Montée : on passe à un plan qui pousse, et on le laisse pousser jusqu'au drop.
    #[allow(clippy::too_many_arguments)] // Même contexte que `decide`.
    fn prepare(
        &mut self,
        frame: &CinemaFrame<'_>,
        flags: CueFlags,
        elapsed: f64,
        spec: Option<usize>,
        editor: &mut Editor,
        catalog: &[Box<dyn ShotSpec>],
        show: &dyn ShowSide,
    ) -> Option<Call> {
        let Some(plan) = show.drop_plan() else {
            self.prepared = false;
            return None;
        };
        if plan.forced || frame.show.breathing {
            return None;
        }
        let pushing = spec
            .and_then(|i| catalog.get(i))
            .is_some_and(|s| PUSH_IDS.contains(&s.id()));
        if !self.prepared && (flags.beat || flags.bar) && elapsed > PREP_GUARD {
            self.prepared = true;
            if !pushing {
                let ids = if plan.shot.is_some() || plan.stunt.is_some() {
                    PUSH_NO_DOLLY
                } else {
                    PUSH_IDS
                };
                if let Some(index) = editor.pick_among(frame, show, catalog, ids, spec) {
                    return Some(Call::Decide(Decision {
                        spec: Some(index),
                        ..Decision::of(PickMode::Normal)
                    }));
                }
            }
        }
        (pushing && elapsed < PREP_HOLD).then_some(Call::Wait)
    }
}
