//! L'état du show vu de l'opérateur : en attente, démarrage, en cours, arrêt.
//!
//! Avec un monde qui possède la séquence 3D (`cf_render::RunSequenced`), `press` n'émet qu'un ordre (`RunOrder`) et
//! l'état suit la phase publiée par le monde, y compris les états transitoires. Sans monde (opérateur seul, tests),
//! `Démarrer` et `Arrêter` basculent directement entre `Waiting` et `Running`.

use bevy::prelude::*;
use cf_render::{Phase, RunOrder, RunPhase, RunSequenced};

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

/// `EguiPrimaryContextPass` : recopie la phase du monde dans l'état de l'opérateur, quand un monde la pilote.
pub(crate) fn sync_with_world(
    phase: Option<Res<RunPhase>>,
    sequenced: Option<Res<RunSequenced>>,
    time: Res<Time<Real>>,
    mut state: ResMut<ShowRunState>,
) {
    let (Some(phase), Some(_)) = (phase, sequenced) else {
        return;
    };
    let mut next = *state;
    next.follow(phase.phase, time.elapsed_secs_f64());
    if *state != next {
        *state = next;
    }
}

/// L'état du show et l'instant de son démarrage (horloge réelle de l'application, en secondes).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct ShowRunState {
    /// Où en est le show.
    pub run: ShowRun,
    started_at_s: Option<f64>,
    /// Un monde pilote la séquence : l'état suit sa phase au lieu de basculer seul.
    sequenced: bool,
}

impl ShowRunState {
    /// Appui sur le bouton principal. Renvoie l'ordre à envoyer au monde : `Start` depuis l'attente, `Stop` depuis le
    /// show en cours, rien pendant une séquence. Sans monde pour piloter la séquence, l'état bascule directement.
    pub fn press(&mut self, now_s: f64) -> Option<RunOrder> {
        let order = match self.run {
            ShowRun::Waiting => Some(RunOrder::Start),
            ShowRun::Running => Some(RunOrder::Stop),
            ShowRun::Starting | ShowRun::Stopping => None,
        };
        if !self.sequenced {
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
        order
    }

    /// Suit la phase publiée par le monde (`sequenced` : un monde pilote la séquence).
    pub fn follow(&mut self, phase: Phase, now_s: f64) {
        self.sequenced = true;
        let run = match phase {
            Phase::Waiting => ShowRun::Waiting,
            Phase::Starting => ShowRun::Starting,
            Phase::Running => ShowRun::Running,
            Phase::Stopping => ShowRun::Stopping,
        };
        if run == self.run {
            return;
        }
        // Le chrono court du début de `Starting` jusqu'à l'attente retrouvée.
        match run {
            ShowRun::Waiting => self.started_at_s = None,
            ShowRun::Starting => self.started_at_s = Some(now_s),
            ShowRun::Running | ShowRun::Stopping => {
                self.started_at_s.get_or_insert(now_s);
            }
        }
        self.run = run;
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
                sequenced: false,
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

#[cfg(test)]
mod world_tests {
    use super::*;

    #[test]
    fn press_returns_the_order_for_the_state() {
        let mut state = ShowRunState::default();
        assert_eq!(state.press(0.0), Some(RunOrder::Start));
        assert_eq!(state.press(1.0), Some(RunOrder::Stop));
    }

    #[test]
    fn press_during_a_transition_sends_nothing() {
        let mut state = ShowRunState::default();
        state.follow(Phase::Starting, 0.0);
        assert_eq!(state.press(1.0), None);
        state.follow(Phase::Stopping, 2.0);
        assert_eq!(state.press(3.0), None);
    }

    #[test]
    fn with_a_world_press_does_not_toggle_alone() {
        let mut state = ShowRunState::default();
        state.follow(Phase::Waiting, 0.0);
        assert_eq!(state.press(1.0), Some(RunOrder::Start));
        assert_eq!(state.run, ShowRun::Waiting, "l'état attend la phase du monde");
    }

    #[test]
    fn state_mirrors_run_phase() {
        let mut state = ShowRunState::default();
        for (phase, run) in [
            (Phase::Starting, ShowRun::Starting),
            (Phase::Running, ShowRun::Running),
            (Phase::Stopping, ShowRun::Stopping),
            (Phase::Waiting, ShowRun::Waiting),
        ] {
            state.follow(phase, 1.0);
            assert_eq!(state.run, run);
        }
    }

    #[test]
    fn button_is_disabled_in_transitions_and_the_chrono_runs_from_the_start() {
        let mut state = ShowRunState::default();
        state.follow(Phase::Starting, 10.0);
        assert!(!state.run.button_enabled());
        assert_eq!(state.elapsed_s(14.0), Some(4.0));
        state.follow(Phase::Running, 28.0);
        assert_eq!(state.elapsed_s(30.0), Some(20.0), "le chrono ne repart pas à Running");
        state.follow(Phase::Stopping, 40.0);
        state.follow(Phase::Waiting, 49.0);
        assert_eq!(state.elapsed_s(50.0), None);
    }

    #[test]
    fn the_sync_system_follows_the_world_only_when_it_drives() {
        let mut app = App::new();
        app.init_resource::<ShowRunState>()
            .init_resource::<Time<Real>>()
            .insert_resource(RunPhase {
                phase: Phase::Starting,
                progress: 0.1,
            })
            .add_systems(Update, sync_with_world);
        app.update();
        assert_eq!(
            app.world().resource::<ShowRunState>().run,
            ShowRun::Waiting,
            "pas de séquenceur : on ne suit pas"
        );
        app.insert_resource(RunSequenced);
        app.update();
        assert_eq!(app.world().resource::<ShowRunState>().run, ShowRun::Starting);
    }
}
