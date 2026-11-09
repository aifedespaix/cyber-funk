//! Le séquenceur : la machine à états pure qui transforme les ordres de l'opérateur en étapes continues.

use cf_render::{Phase, RunOrder};

use super::handover::{ArrivalStages, DoubleStages, HandoverFrame, handover_stages};
use super::timeline::{CRUISE_MPS, RunStages, START_S, STOP_S, starting_cap, starting_stages, stopping_cap, stopping_stages};
use super::variant::HandoverVariant;
use crate::intro::{IntroInput, IntroStages, PreIntro, WindowFront};

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
    /// L'ancienne voiture pendant un changement de DJ (double visuel).
    pub double: DoubleStages,
    /// La nouvelle voiture (la réelle) pendant un changement de DJ.
    pub arrival: ArrivalStages,
    /// Rang du DJ visé pendant un changement de DJ.
    pub handover_slot: Option<usize>,
    /// L'instant et la variante du changement de DJ en cours.
    pub handover: Option<HandoverFrame>,
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
    /// Rang du DJ actuel et nombre de DJs de la liste, lus dans le show par le monde.
    current_slot: usize,
    slot_count: usize,
    /// DJ visé par la passation en cours.
    target: Option<usize>,
    /// La variante tirée pour la passation en cours.
    variant: Option<HandoverVariant>,
    /// Nombre de passations déjà lancées (graine des variantes) et style de cadrage de la dernière.
    handovers: u32,
    last_style: Option<u8>,
    /// L'opérateur demande moins de mouvement : plus de glitch ni de clignotement.
    reduce_motion: bool,
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
            current_slot: 0,
            slot_count: 0,
            target: None,
            variant: None,
            handovers: 0,
            last_style: None,
            reduce_motion: false,
        }
    }

    /// Dit au séquenceur à quelle vitesse roule la voiture : un arrêt freine depuis cette vitesse.
    pub fn set_speed(&mut self, speed_mps: f32) {
        self.speed_mps = if speed_mps.is_finite() { speed_mps.max(0.0) } else { 0.0 };
    }

    /// Dit au séquenceur où en est la liste des DJs : le DJ actuel et leur nombre.
    pub fn set_slot_info(&mut self, current: usize, count: usize) {
        self.current_slot = current;
        self.slot_count = count;
    }

    /// Réglage « moins de mouvement » : supprime le glitch du nom et de la voiture qui s'éteint.
    pub fn set_reduce_motion(&mut self, reduce: bool) {
        self.reduce_motion = reduce;
    }

    /// Le DJ visé par un ordre de passation, s'il est valable : au moins deux DJs, une cible dans la liste, autre que
    /// le DJ actuel ; sans cible précise, le DJ qui suit l'actuel (rien après le dernier).
    fn handover_target(&self, slot: Option<usize>) -> Option<usize> {
        let target = slot.or_else(|| self.current_slot.checked_add(1))?;
        (self.slot_count > 1 && target < self.slot_count && target != self.current_slot).then_some(target)
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
            (Phase::Running, Some(RunOrder::Handover { slot })) => {
                if let Some(target) = self.handover_target(slot) {
                    self.enter(Phase::Handover);
                    self.target = Some(target);
                    let variant = HandoverVariant::new(self.handovers, target, self.last_style);
                    self.handovers = self.handovers.wrapping_add(1);
                    self.last_style = Some(variant.framing.style);
                    self.variant = Some(variant);
                }
            }
            _ => {}
        }
        let dt_s = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        match self.phase {
            // La voiture, invisible, reste au point de départ : le plan d'attente la filme dès son apparition.
            Phase::Waiting => out(Phase::Waiting, 1.0, RunStages::WAITING, Some(0.0), None),
            Phase::Running => out(Phase::Running, 1.0, RunStages::RUNNING, None, None),
            Phase::Handover => self.update_handover(dt_s),
            Phase::Starting => {
                self.t_s = (self.t_s + dt_s).min(START_S);
                let stages = starting_stages(self.t_s, self.reduce_motion);
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
                    None,
                )
            }
        }
    }

    fn update_handover(&mut self, dt_s: f32) -> RunOutput {
        let Some(variant) = self.variant else {
            // Impossible : la variante est tirée à l'entrée dans la phase. Repli sûr : rendre la main tout de suite.
            self.phase = Phase::Running;
            return out(Phase::Running, 1.0, RunStages::RUNNING, None, None);
        };
        let duration = variant.duration_s();
        self.t_s = (self.t_s + dt_s).min(duration);
        let (stages, double, arrival) = handover_stages(self.t_s, &variant, self.reduce_motion);
        let progress = self.t_s / duration;
        let frame = HandoverFrame {
            t_s: self.t_s,
            variant,
        };
        let target = self.target;
        if self.t_s >= duration {
            self.phase = Phase::Running;
            self.t_s = 0.0;
            self.target = None;
            self.variant = None;
        }
        RunOutput {
            phase: Phase::Handover,
            progress,
            stages,
            speed_cap_mps: Some(CRUISE_MPS),
            front: None,
            double,
            arrival,
            handover_slot: target,
            handover: Some(frame),
        }
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.t_s = 0.0;
        self.front = PreIntro::new();
        self.brake_from_mps = self.speed_mps;
        self.target = None;
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
        double: DoubleStages::NONE,
        arrival: ArrivalStages::NONE,
        handover_slot: None,
        handover: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::handover::{DoubleStages, HANDOVER_S};
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

    // --- Changement de DJ ---

    fn running(slot: usize, count: usize) -> RunSequencer {
        let mut seq = RunSequencer::new();
        seq.update(Some(RunOrder::Start), 100.0, false);
        seq.update(None, 100.0, false);
        seq.set_slot_info(slot, count);
        seq
    }

    #[test]
    fn handover_from_running_enters_handover_and_targets_the_next_dj() {
        let mut seq = running(0, 3);
        let o = seq.update(Some(RunOrder::Handover { slot: None }), DT, false);
        assert_eq!((o.phase, o.handover_slot), (Phase::Handover, Some(1)));
        let mut seq = running(0, 3);
        let o = seq.update(Some(RunOrder::Handover { slot: Some(2) }), DT, false);
        assert_eq!(o.handover_slot, Some(2));
    }

    #[test]
    fn handover_is_ignored_unless_running() {
        let mut seq = RunSequencer::new();
        seq.set_slot_info(0, 3);
        assert_eq!(
            seq.update(Some(RunOrder::Handover { slot: None }), DT, false).phase,
            Phase::Waiting
        );
        seq.update(Some(RunOrder::Start), DT, false);
        assert_eq!(
            seq.update(Some(RunOrder::Handover { slot: None }), DT, false).phase,
            Phase::Starting
        );
        let mut seq = running(0, 3);
        seq.update(Some(RunOrder::Stop), DT, false);
        assert_eq!(
            seq.update(Some(RunOrder::Handover { slot: None }), DT, false).phase,
            Phase::Stopping
        );
    }

    #[test]
    fn handover_is_ignored_with_one_dj_at_the_last_dj_or_toward_the_current_dj() {
        for (current, count, order) in [
            (0, 1, RunOrder::Handover { slot: None }),
            (2, 3, RunOrder::Handover { slot: None }),
            (1, 3, RunOrder::Handover { slot: Some(1) }),
            (0, 3, RunOrder::Handover { slot: Some(3) }),
        ] {
            let mut seq = running(current, count);
            assert_eq!(
                seq.update(Some(order), DT, false).phase,
                Phase::Running,
                "{current}/{count} {order:?}"
            );
        }
    }

    #[test]
    fn orders_are_ignored_during_a_handover() {
        let mut seq = running(0, 3);
        seq.update(Some(RunOrder::Handover { slot: None }), DT, false);
        assert_eq!(seq.update(Some(RunOrder::Stop), DT, false).phase, Phase::Handover);
        let o = seq.update(Some(RunOrder::Handover { slot: Some(2) }), DT, false);
        assert_eq!(o.handover_slot, Some(1), "la cible ne change pas en cours de route");
    }

    #[test]
    fn handover_ends_in_running_after_its_duration() {
        let mut seq = running(0, 3);
        seq.update(Some(RunOrder::Handover { slot: None }), DT, false);
        run_for(&mut seq, HANDOVER_S - 0.5);
        assert_eq!(seq.phase(), Phase::Handover);
        run_for(&mut seq, 1.0);
        assert_eq!(seq.phase(), Phase::Running);
        let o = seq.update(None, DT, false);
        assert!(o.stages == RunStages::RUNNING && o.double == DoubleStages::NONE && o.handover_slot.is_none());
    }

    #[test]
    fn the_new_car_is_allowed_to_catch_up_then_returns_to_cruise() {
        let mut seq = running(0, 2);
        seq.update(Some(RunOrder::Handover { slot: None }), 0.0, false);
        let (mut peak, mut last) = (0.0_f32, 0.0_f32);
        for _ in 0..(HANDOVER_S * 60.0) as u32 {
            last = seq.update(None, DT, false).speed_cap_mps.unwrap_or(0.0);
            peak = peak.max(last);
        }
        assert!(peak > CRUISE_MPS + 5.0, "{peak}");
        assert!(last <= CRUISE_MPS + 0.01, "{last}");
    }

    #[test]
    fn handover_outputs_are_finite_and_in_range_over_many_cycles() {
        let mut seq = running(0, 3);
        for _ in 0..30 {
            seq.update(Some(RunOrder::Handover { slot: None }), DT, true);
            for _ in 0..(HANDOVER_S * 60.0) as u32 + 5 {
                let o = seq.update(None, DT, true);
                check(o);
                assert!(o.arrival.gap_behind_m.is_finite() && o.double.presence.is_finite());
            }
            seq.set_slot_info(0, 3);
        }
    }

    #[test]
    fn start_handovers_and_stop_return_to_the_identical_waiting_state() {
        let mut seq = RunSequencer::new();
        let baseline = seq.update(None, DT, false);
        for _ in 0..100 {
            seq.update(Some(RunOrder::Start), 100.0, false);
            seq.update(None, 100.0, false);
            for _ in 0..3 {
                seq.set_slot_info(0, 3);
                seq.update(Some(RunOrder::Handover { slot: None }), DT, false);
                seq.update(None, 100.0, false);
            }
            seq.update(Some(RunOrder::Stop), DT, false);
            seq.update(None, 100.0, false);
        }
        assert_eq!(seq.update(None, DT, false), baseline);
    }
}
