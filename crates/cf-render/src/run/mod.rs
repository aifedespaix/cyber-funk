//! Le contrat entre l'opérateur et le monde pour démarrer et arrêter le show.
//!
//! L'opérateur envoie un [`RunOrder`] ; le monde (qui possède la séquence 3D) tient [`RunPhase`] à jour. Sans
//! séquenceur installé (exemples, outils, tests), la phase reste `Running` : rien ne change.

use bevy::prelude::*;

/// Part de la séquence de démarrage à partir de laquelle le show s'arme (12 s sur 29 s, au début du portique) : avant,
/// la voiture se forme et roule sans qu'aucun cue ne parte.
pub const ARM_AT: f32 = 12.0 / 29.0;

/// Ordre de l'opérateur au monde.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOrder {
    /// Lancer la séquence de démarrage (depuis l'attente seulement).
    Start,
    /// Lancer la séquence d'arrêt (depuis le show en cours seulement).
    Stop,
    /// Lancer le changement de DJ (depuis le show en cours seulement). `None` : le DJ qui suit l'actuel dans la
    /// liste ; `Some(i)` : le DJ de rang `i`.
    Handover {
        /// Rang du DJ visé dans la liste du show.
        slot: Option<usize>,
    },
}

/// Le DJ visé par la passation en cours. Écrit par le monde seulement ; `None` hors passation.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunHandover(pub Option<usize>);

/// Marque qu'un séquenceur pilote [`RunPhase`] : l'opérateur en suit alors la phase au lieu de basculer seul.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct RunSequenced;

/// Où en est le show, du point de vue du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Scène d'attente : caméra sur la route, rien ne joue.
    Waiting,
    /// Séquence de démarrage.
    Starting,
    /// Le show joue.
    #[default]
    Running,
    /// Séquence d'arrêt.
    Stopping,
    /// Changement de DJ : le quartier se replie, la nouvelle voiture double l'ancienne, le portique annonce le nouveau DJ.
    Handover,
}

/// La phase courante et l'avancement (0 à 1) de la séquence en cours. Écrite par le monde seulement.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RunPhase {
    /// Phase courante.
    pub phase: Phase,
    /// Avancement de la phase (1 hors séquence).
    pub progress: f32,
}

impl Default for RunPhase {
    fn default() -> Self {
        Self {
            phase: Phase::Running,
            progress: 1.0,
        }
    }
}

impl RunPhase {
    /// Le chef du spectacle a-t-il le droit de jouer ?
    #[must_use]
    pub fn show_armed(&self) -> bool {
        match self.phase {
            Phase::Running | Phase::Stopping | Phase::Handover => true,
            Phase::Starting => self.progress >= ARM_AT,
            Phase::Waiting => false,
        }
    }
}

/// Condition d'exécution : le show peut-il jouer ? Vrai sans séquenceur.
#[must_use]
pub fn show_armed(phase: Option<Res<RunPhase>>) -> bool {
    phase.is_none_or(|p| p.show_armed())
}

/// Enregistre le contrat (idempotent : l'opérateur et le monde peuvent tous deux l'ajouter).
#[derive(Debug, Default, Clone, Copy)]
pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        app.add_message::<RunOrder>()
            .init_resource::<RunPhase>()
            .init_resource::<RunHandover>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(phase: Phase, progress: f32) -> RunPhase {
        RunPhase { phase, progress }
    }

    #[test]
    fn default_phase_is_running() {
        assert_eq!(RunPhase::default().phase, Phase::Running);
    }

    #[test]
    fn show_armed_without_resource_is_true() {
        let mut app = App::new();
        app.add_systems(Update, |p: Option<Res<RunPhase>>| assert!(show_armed(p)));
        app.update();
    }

    #[test]
    fn show_armed_false_while_waiting() {
        assert!(!at(Phase::Waiting, 0.0).show_armed());
    }

    #[test]
    fn show_armed_in_starting_only_after_arm_point() {
        assert!(!at(Phase::Starting, ARM_AT - 0.01).show_armed());
        assert!(at(Phase::Starting, ARM_AT).show_armed());
        assert!(at(Phase::Stopping, 0.2).show_armed());
    }

    #[test]
    fn show_armed_during_a_handover_whatever_the_progress() {
        assert!(at(Phase::Handover, 0.0).show_armed());
        assert!(at(Phase::Handover, 1.0).show_armed());
    }

    #[test]
    fn handover_order_is_copy_and_comparable() {
        let a = RunOrder::Handover { slot: Some(2) };
        let b = a;
        assert_eq!(a, b);
        assert_ne!(a, RunOrder::Handover { slot: None });
    }

    #[test]
    fn handover_target_defaults_to_none() {
        assert_eq!(RunHandover::default(), RunHandover(None));
    }

    #[test]
    fn plugin_is_idempotent() {
        let mut app = App::new();
        app.add_plugins((RunPlugin, RunPlugin));
        app.update();
    }
}
