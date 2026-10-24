//! Le plafond de cadence des coupes, appliqué à toutes les coupes du réalisateur (voir [`crate::choreo`]).
//!
//! Deux sources de coupes : la transition `Cut` que le réalisateur lance entre deux plans, et le saut que certains plans
//! s'infligent à eux-mêmes (`strobe` change d'angle à chaque temps sans transition). La première est adoucie en fondu
//! quand le plafond est atteint ; le second est retardé : le plan garde sa pose précédente, portée par la voiture,
//! jusqu'à ce que la fenêtre d'une seconde se libère.
//!
//! Le saut d'un plan est repéré sur la pose du plan lui-même, avant tout mélange de transition : un plan qui coupe
//! pendant un fondu fabrique sinon des coupes atténuées mais bien réelles, hors de portée d'un contrôle sur la pose
//! finale seule.

use super::{CinemaDirector, DirectorStats, Slot};
use crate::choreo::{CutLimiter, is_jump};
use crate::transition::SWOOP_DISTANCE;
use crate::types::TransitionKind;
use crate::world::CinemaFrame;

impl CinemaDirector {
    /// Une transition `Cut` demandée à `now` : admise si le plafond le permet, sinon un fondu (une envolée pour les longs
    /// trajets). Les autres entrées ne sont pas des coupes.
    pub(super) fn cap_cut(&mut self, kind: TransitionKind, now: f64, distance: f64) -> TransitionKind {
        if kind != TransitionKind::Cut {
            return kind;
        }
        if self.limiter.try_cut(now) {
            self.stats.cuts += 1;
            return kind;
        }
        self.stats.cuts_held += 1;
        if distance > SWOOP_DISTANCE {
            TransitionKind::Swoop
        } else {
            TransitionKind::Blend
        }
    }
}

impl Slot {
    /// À appeler après chaque mise à jour du plan : si sa pose vient de sauter (changement d'angle), la coupe passe par
    /// le plafond. Refusée, la pose précédente est tenue ; le plan, lui, continue et recoupera dès que la fenêtre le
    /// permet.
    pub(super) fn steady(&mut self, limiter: &mut CutLimiter, stats: &mut DirectorStats, frame: &CinemaFrame<'_>) {
        if let Some((before, before_car)) = self.prev
            && is_jump(&before, before_car, &self.pose, frame.car, frame.dt)
        {
            if limiter.try_cut(frame.time) {
                stats.cuts += 1;
            } else {
                stats.cuts_held += 1;
                let mut held = before;
                held.position += frame.car - before_car;
                self.pose = held;
            }
        }
        self.prev = Some((self.pose, frame.car));
    }
}
