//! Le choix de l'entrée d'un plan : coupe, fondu, envolée ou panoramique éclair.

use crate::editor::PickMode;
use crate::pacing::grid_beats;
use crate::transition::SWOOP_DISTANCE;
use crate::types::TransitionKind;
use crate::world::CinemaFrame;

use super::CinemaDirector;

/// Une coupe franche « waouh » au plus toutes les 45 s : le reste du temps, la caméra voyage.
const WOW_GAP: f64 = 45.0;

impl CinemaDirector {
    /// La caméra voyage : fondu en arc, envolée pour les longs trajets. La coupe franche est un effet réservé : un
    /// drop, l'astre qui tombe, la plongée vers l'habitacle ou une figure, et pas plus d'une toutes les 45 s (sauf
    /// l'astre).
    pub(super) fn choose_kind(
        &self,
        frame: &CinemaFrame<'_>,
        index: usize,
        mode: PickMode,
        distance: f64,
        cabin: bool,
    ) -> TransitionKind {
        let Some(spec) = self.catalog.get(index) else {
            return TransitionKind::Blend;
        };
        let show = frame.show;
        // Respiration : jamais de coupe ni de panoramique éclair, la caméra glisse.
        if show.active && show.breathing {
            return if distance > SWOOP_DISTANCE {
                TransitionKind::Swoop
            } else {
                TransitionKind::Blend
            };
        }
        // Une animation de la voiture n'attend pas : panoramique éclair.
        if spec.reactive() {
            return TransitionKind::Whip;
        }
        // En spectacle, la grille serre le rythme : les coupes franches reviennent plus souvent, les fondus raccourcissent.
        let grid = grid_beats(frame);
        let wow_gap = if grid > 0 {
            WOW_GAP * (f64::from(grid) / 16.0).min(1.0)
        } else {
            WOW_GAP
        };
        let wow = frame.time - self.last_cut >= wow_gap && !self.calm;
        let tight = grid > 0 && grid <= 4;
        let wanted = spec.entry() == TransitionKind::Cut || cabin || mode == PickMode::Drop || tight;
        if spec.id() == "meteor-impact-watch" || (wow && wanted && matches!(mode, PickMode::Drop | PickMode::Normal)) {
            return TransitionKind::Cut;
        }
        if tight && !spec.scripted() {
            return TransitionKind::Whip;
        }
        if distance > SWOOP_DISTANCE || (mode == PickMode::Ended && distance > 20.0 && self.editor.after_special) {
            return TransitionKind::Swoop;
        }
        if spec.entry() == TransitionKind::Swoop {
            TransitionKind::Swoop
        } else {
            TransitionKind::Blend
        }
    }
}
