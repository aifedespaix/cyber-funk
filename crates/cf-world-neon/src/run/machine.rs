//! Le séquenceur : la machine à états pure qui transforme les ordres de l'opérateur en étapes continues.

use cf_render::{Phase, RunOrder};

use super::timeline::{
    CRUISE_MPS, RunStages, START_S, STOP_S, starting_cap, starting_stages, stopping_cap, stopping_stages,
};
use crate::intro::{FRONT_WIDTH_M, IntroInput, IntroStages, PreIntro, WindowFront};

/// Portée du front des fenêtres à l'arrêt, quand toutes sont allumées (m).
const SHUTDOWN_REACH_M: f32 = 950.0;

/// Ce que la séquence demande au monde cette image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunOutput {
    /// Phase rendue cette image.
    pub phase: Phase,
    /// Avancement de la phase, 0 à 1 (1 hors séquence).
    pub progress: f32,
    /// Les étapes à appliquer.
    pub stages: RunStages,
    /// Plafond de vitesse (m/s) imposé à la voiture, s'il y en a un.
    pub speed_cap_mps: Option<f32>,
    /// Front des fenêtres allumées.
    pub front: Option<WindowFront>,
}

/// La machine à états du démarrage et de l'arrêt.
#[derive(Debug, Clone, Copy)]
pub struct RunSequencer {
    phase: Phase,
    t_s: f32,
    front: PreIntro,
    /// Vitesse réelle de la voiture (m/s), lue par le monde avant chaque image.
    speed_mps: f32,
    /// Vitesse à laquelle l'arrêt a commencé.
    brake_from_mps: f32,
}

impl Default for RunSequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl RunSequencer {
    /// Au repos, en attente.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            phase: Phase::Waiting,
            t_s: 0.0,
            front: PreIntro::new(),
            speed_mps: CRUISE_MPS,
            brake_from_mps: CRUISE_MPS,
        }
    }

    /// Dit au séquenceur à quelle vitesse roule la voiture : un arrêt freine depuis cette vitesse.
    pub fn set_speed(&mut self, speed_mps: f32) {
        self.speed_mps = if speed_mps.is_finite() { speed_mps.max(0.0) } else { 0.0 };
    }

    /// Phase courante (après la dernière image).
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Avance d'une image. Un ordre n'est pris en compte que depuis `Waiting` (`Start`) ou `Running` (`Stop`) : pendant
    /// une séquence il est ignoré, pour qu'un double appui ne la relance pas.
    pub fn update(&mut self, order: Option<RunOrder>, dt_s: f32, kick: bool) -> RunOutput {
        match (self.phase, order) {
            (Phase::Waiting, Some(RunOrder::Start)) => self.enter(Phase::Starting),
            (Phase::Running, Some(RunOrder::Stop)) => self.enter(Phase::Stopping),
            _ => {}
        }
        let dt_s = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        match self.phase {
            // La voiture, invisible, reste au point de départ : le plan d'attente la filme dès son apparition.
            Phase::Waiting => out(Phase::Waiting, 1.0, RunStages::WAITING, Some(0.0), None),
            // Rempli par la tâche 3 : la passation n'est pas encore atteignable.
            Phase::Running | Phase::Handover => out(Phase::Running, 1.0, RunStages::RUNNING, None, None),
            Phase::Starting => {
                self.t_s = (self.t_s + dt_s).min(START_S);
                let stages = starting_stages(self.t_s);
                let tracker = IntroStages {
                    windows: stages.windows,
                    ..IntroStages::DONE
                };
                let front = self.front.front(
                    &tracker,
                    &IntroInput {
                        level: None,
                        dt_s,
                        kick,
                        energy: 0.0,
                    },
                );
                let progress = self.t_s / START_S;
                if self.t_s >= START_S {
                    self.phase = Phase::Running;
                }
                out(Phase::Starting, progress, stages, Some(starting_cap(&stages)), front)
            }
            Phase::Stopping => {
                self.t_s = (self.t_s + dt_s).min(STOP_S);
                let stages = stopping_stages(self.t_s);
                let front = (stages.windows < 1.0).then_some(WindowFront {
                    ahead_m: SHUTDOWN_REACH_M * stages.windows,
                    edge_m: FRONT_WIDTH_M,
                });
                let progress = self.t_s / STOP_S;
                if self.t_s >= STOP_S {
                    self.phase = Phase::Waiting;
                    self.t_s = 0.0;
                }
                out(
                    Phase::Stopping,
                    progress,
                    stages,
                    Some(stopping_cap(progress * STOP_S, self.brake_from_mps)),
                    front,
                )
            }
        }
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.t_s = 0.0;
        self.front = PreIntro::new();
        self.brake_from_mps = self.speed_mps;
    }
}

fn out(
    phase: Phase,
    progress: f32,
    stages: RunStages,
    speed_cap_mps: Option<f32>,
    front: Option<WindowFront>,
) -> RunOutput {
    RunOutput {
        phase,
        progress,
        stages,
        speed_cap_mps,
        front,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::timeline::brake_rate;

    const DT: f32 = 1.0 / 60.0;

    fn run_for(seq: &mut RunSequencer, seconds: f32) -> RunOutput {
        let mut last = seq.update(None, 0.0, false);
        let mut t = 0.0;
        while t < seconds {
            last = seq.update(None, DT, false);
            t += DT;
        }
        last
    }

    #[test]
    fn start_from_waiting_enters_starting() {
        let mut seq = RunSequencer::new();
        assert_eq!(seq.update(Some(RunOrder::Start), DT, false).phase, Phase::Starting);
    }

    #[test]
    fn stop_from_waiting_is_ignored() {
        let mut seq = RunSequencer::new();
        assert_eq!(seq.update(Some(RunOrder::Stop), DT, false).phase, Phase::Waiting);
    }

    #[test]
    fn orders_ignored_during_starting() {
        let mut seq = RunSequencer::new();
        seq.update(Some(RunOrder::Start), DT, false);
        assert_eq!(seq.update(Some(RunOrder::Stop), DT, false).phase, Phase::Starting);
        let before = seq.t_s;
        seq.update(Some(RunOrder::Start), DT, false);
        assert!(seq.t_s > before, "un second Start ne doit pas relancer la séquence");
    }

    #[test]
    fn orders_ignored_during_stopping() {
        let mut seq = RunSequencer::new();
        seq.update(Some(RunOrder::Start), 100.0, false);
        seq.update(Some(RunOrder::Stop), DT, false);
        assert_eq!(seq.update(Some(RunOrder::Start), DT, false).phase, Phase::Stopping);
    }

    #[test]
    fn starting_ends_in_running_after_18s() {
        let mut seq = RunSequencer::new();
        seq.update(Some(RunOrder::Start), DT, false);
        run_for(&mut seq, START_S - 0.5);
        assert_eq!(seq.phase(), Phase::Starting);
        run_for(&mut seq, 1.0);
        assert_eq!(seq.phase(), Phase::Running);
        assert_eq!(seq.update(None, DT, false).stages, RunStages::RUNNING);
    }

    #[test]
    fn stopping_ends_in_waiting_after_9s() {
        let mut seq = RunSequencer::new();
        seq.update(Some(RunOrder::Start), 100.0, false);
        seq.update(Some(RunOrder::Stop), DT, false);
        run_for(&mut seq, STOP_S - 0.5);
        assert_eq!(seq.phase(), Phase::Stopping);
        run_for(&mut seq, 1.0);
        assert_eq!(seq.phase(), Phase::Waiting);
        assert_eq!(seq.update(None, DT, false).stages, RunStages::WAITING);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Constantes exactes de la table du spec.
    fn waiting_stages_match_spec_table() {
        let w = RunStages::WAITING;
        assert!((w.road - 0.25).abs() < f32::EPSILON && (w.sky - 0.3).abs() < f32::EPSILON);
        assert!(w.build == 0.0 && w.windows == 0.0 && w.lights == 0.0 && w.car_presence == 0.0 && w.music == 0.0);
        assert!(w.camera_waiting);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Constantes exactes.
    fn running_stages_are_all_in_place() {
        let r = RunStages::RUNNING;
        assert!(r.road == 1.0 && r.build == 1.0 && r.sign == 1.0 && r.car_presence == 1.0 && r.collapse == 0.0);
    }

    #[test]
    fn stages_are_monotonic_while_starting() {
        let mut seq = RunSequencer::new();
        let mut prev = seq.update(Some(RunOrder::Start), 0.0, false).stages;
        for _ in 0..(START_S * 60.0) as u32 {
            let s = seq.update(None, DT, false).stages;
            assert!(s.road >= prev.road && s.build >= prev.build && s.sky >= prev.sky && s.sign >= prev.sign);
            assert!(s.car_presence >= prev.car_presence && s.lights >= prev.lights && s.music >= prev.music);
            prev = s;
        }
    }

    fn check(o: RunOutput) {
        let s = o.stages;
        for v in [
            s.road,
            s.lights,
            s.build,
            s.windows,
            s.sky,
            s.sign,
            s.drive,
            s.collapse,
            s.car_presence,
            s.music,
        ] {
            assert!(v.is_finite() && (0.0..=1.0).contains(&v), "{s:?}");
        }
        assert!(o.progress.is_finite() && (0.0..=1.0).contains(&o.progress));
    }

    #[test]
    fn no_nan_and_all_in_unit_range_over_100_cycles() {
        let mut seq = RunSequencer::new();
        for _ in 0..100 {
            seq.update(Some(RunOrder::Start), DT, true);
            for _ in 0..(START_S * 60.0) as u32 + 5 {
                check(seq.update(None, DT, true));
            }
            seq.update(Some(RunOrder::Stop), DT, false);
            for _ in 0..(STOP_S * 60.0) as u32 + 5 {
                check(seq.update(None, DT, false));
            }
        }
    }

    #[test]
    fn hundred_cycles_return_to_identical_waiting_state() {
        let mut seq = RunSequencer::new();
        let baseline = seq.update(None, DT, false);
        for _ in 0..100 {
            seq.update(Some(RunOrder::Start), 100.0, false);
            seq.update(None, 100.0, false);
            seq.update(Some(RunOrder::Stop), DT, false);
            seq.update(None, 100.0, false);
        }
        assert_eq!(seq.update(None, DT, false), baseline);
    }

    #[test]
    fn huge_dt_finishes_phase_without_skipping() {
        let mut seq = RunSequencer::new();
        let o = seq.update(Some(RunOrder::Start), 1000.0, false);
        assert_eq!(o.phase, Phase::Starting, "la dernière image de la séquence est rendue");
        assert_eq!(seq.phase(), Phase::Running);
    }

    #[test]
    fn braking_never_exceeds_the_comfort_deceleration() {
        for from in [16.0_f32, 40.0] {
            let mut seq = RunSequencer::new();
            seq.update(Some(RunOrder::Start), 100.0, false);
            seq.update(None, 100.0, false);
            seq.set_speed(from);
            seq.update(Some(RunOrder::Stop), 0.0, false);
            let limit = brake_rate(from);
            let (mut prev, mut stopped_at): (Option<f32>, Option<f32>) = (None, None);
            for k in 0..(STOP_S * 60.0) as u32 {
                let cap = seq.update(None, DT, false).speed_cap_mps.unwrap_or(0.0);
                if let Some(p) = prev {
                    assert!((p - cap) / DT <= limit + 0.01, "{from}: {p} -> {cap}");
                }
                if cap == 0.0 && stopped_at.is_none() {
                    stopped_at = Some(k as f32 * DT);
                }
                prev = Some(cap);
            }
            assert!(
                stopped_at.is_some_and(|t| t <= 4.6),
                "{from} m/s doit être arrêtée en ~4,5 s : {stopped_at:?}"
            );
        }
    }

    #[test]
    fn the_stop_starts_from_the_real_speed_not_from_cruise() {
        let mut seq = RunSequencer::new();
        seq.update(Some(RunOrder::Start), 100.0, false);
        seq.update(None, 100.0, false);
        seq.set_speed(40.0);
        let first = seq.update(Some(RunOrder::Stop), 0.0, false);
        assert!(
            (first.speed_cap_mps.unwrap_or(0.0) - 40.0).abs() < 0.01,
            "pas de coup de frein au départ"
        );
    }
}
