//! Le mode de la fenêtre opérateur, déduit de l'état du show : Live dès que le show démarre, Préparation sinon.
//!
//! Il n'y a plus de bascule manuelle : l'opérateur ne choisit pas un « mode », il démarre ou arrête le show. Le mode
//! reste lu par l'onglet Debug (pas d'ordre de quartier en plein set), la caméra libre, l'aperçu et la mise à jour
//! automatique du binaire.

use bevy::prelude::*;

use crate::run_state::ShowRunState;

/// Le mode de la fenêtre opérateur.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatorMode {
    /// Le show n'est pas lancé : réglages et essais permis.
    #[default]
    Preparation,
    /// Le show joue (ou démarre, ou s'arrête) : rien ne doit toucher au monde ni redémarrer l'application.
    Live,
}

impl OperatorMode {
    /// Le mode qui correspond à l'état du show.
    #[must_use]
    pub(crate) fn of(run: &ShowRunState) -> Self {
        if run.is_active() { Self::Live } else { Self::Preparation }
    }
}

/// Recopie l'état du show dans le mode, sans réveiller les lecteurs quand rien ne change.
pub(crate) fn sync_mode(run: Res<ShowRunState>, mut mode: ResMut<OperatorMode>) {
    mode.set_if_neq(OperatorMode::of(&run));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_state::ShowRun;

    #[test]
    fn le_mode_suit_l_etat_du_show() {
        let mut run = ShowRunState::default();
        assert_eq!(OperatorMode::of(&run), OperatorMode::Preparation);
        run.press(0.0);
        assert_eq!(run.run, ShowRun::Running);
        assert_eq!(OperatorMode::of(&run), OperatorMode::Live);
        run.press(1.0);
        assert_eq!(OperatorMode::of(&run), OperatorMode::Preparation);
    }
}
