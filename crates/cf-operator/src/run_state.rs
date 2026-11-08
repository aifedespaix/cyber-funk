//! L'état du show vu de l'opérateur : en attente, démarrage, en cours, arrêt.
//!
//! Provisoire : tant que la séquence de démarrage et d'arrêt du monde 3D n'existe pas, `Démarrer` et `Arrêter`
//! basculent directement entre `Waiting` et `Running`, sans passer par les états transitoires. Ceux-ci existent déjà
//! pour que l'interface (bouton grisé, libellé) n'ait pas à changer quand la séquence sera branchée.

use bevy::prelude::*;

/// Où en est le show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShowRun {
    /// Scène d'attente : la caméra est fixe sur la route, rien n'est lancé.
    #[default]
    Waiting,
    /// Séquence de démarrage (voiture, quartier, introduction du DJ).
    Starting,
    /// Le show joue.
    Running,
    /// Séquence d'arrêt (quartier qui se désynchronise, voiture qui s'éteint).
    Stopping,
}

impl ShowRun {
    /// Le bouton principal est-il utilisable ? Pas pendant une séquence : un second clic ne doit pas la relancer.
    #[must_use]
    pub fn button_enabled(self) -> bool {
        matches!(self, Self::Waiting | Self::Running)
    }
}

/// L'état du show et l'instant de son démarrage (horloge réelle de l'application, en secondes).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct ShowRunState {
    /// Où en est le show.
    pub run: ShowRun,
    started_at_s: Option<f64>,
}

impl ShowRunState {
    /// Appui sur le bouton principal : démarre depuis l'attente, arrête depuis le show en cours, sinon ne fait rien.
    pub fn press(&mut self, now_s: f64) {
        match self.run {
            ShowRun::Waiting => {
                self.run = ShowRun::Running;
                self.started_at_s = Some(now_s);
            }
            ShowRun::Running => {
                self.run = ShowRun::Waiting;
                self.started_at_s = None;
            }
            ShowRun::Starting | ShowRun::Stopping => {}
        }
    }

    /// Le show tourne-t-il (ou est-il en train de démarrer ou de s'arrêter) ? Sert à ne pas mettre l'application à
    /// jour en plein set.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.run != ShowRun::Waiting
    }

    /// Secondes écoulées depuis le démarrage ; `None` en attente.
    #[must_use]
    pub fn elapsed_s(&self, now_s: f64) -> Option<f64> {
        self.started_at_s.map(|start| (now_s - start).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demarrer_passe_en_cours() {
        let mut state = ShowRunState::default();
        state.press(10.0);
        assert_eq!(state.run, ShowRun::Running);
        assert!(state.is_active());
    }

    #[test]
    fn arreter_repasse_en_attente() {
        let mut state = ShowRunState::default();
        state.press(10.0);
        state.press(20.0);
        assert_eq!(state.run, ShowRun::Waiting);
        assert!(!state.is_active());
        assert_eq!(state.elapsed_s(30.0), None);
    }

    #[test]
    fn appui_ignore_pendant_les_etats_transitoires() {
        for transitional in [ShowRun::Starting, ShowRun::Stopping] {
            let mut state = ShowRunState {
                run: transitional,
                started_at_s: Some(1.0),
            };
            state.press(5.0);
            assert_eq!(state.run, transitional);
            assert!(!transitional.button_enabled());
        }
    }

    #[test]
    fn duree_ecoulee_depuis_le_demarrage() {
        let mut state = ShowRunState::default();
        assert_eq!(state.elapsed_s(5.0), None);
        state.press(10.0);
        assert_eq!(state.elapsed_s(52.5), Some(42.5));
        assert_eq!(state.elapsed_s(3.0), Some(0.0), "jamais négative");
    }
}
