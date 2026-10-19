//! Compteurs du thread d'analyse, lisibles depuis n'importe quel thread (atomiques), et fenêtre de mesure de la
//! latence callback → publication.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Latence visée entre la livraison d'un bloc par le callback et la publication du pas qui le contient.
pub const LATENCY_TARGET: Duration = Duration::from_millis(15);

/// Compteurs partagés du thread d'analyse.
#[derive(Debug, Default)]
pub struct AnalysisStats {
    pub(super) steps: AtomicU64,
    pub(crate) dropped_events: AtomicU64,
    pub(super) feed_errors: AtomicU64,
    pub(super) input_dropped: AtomicU64,
    pub(super) latency_last_us: AtomicU64,
    pub(super) latency_max_us: AtomicU64,
    pub(super) latency_sum_us: AtomicU64,
    pub(super) latency_count: AtomicU64,
}

impl AnalysisStats {
    /// Instantané cohérent à quelques pas près (lectures `Relaxed` indépendantes).
    #[must_use]
    pub fn snapshot(&self) -> AnalysisStatsSnapshot {
        let count = self.latency_count.load(Ordering::Relaxed);
        let sum = self.latency_sum_us.load(Ordering::Relaxed);
        AnalysisStatsSnapshot {
            steps: self.steps.load(Ordering::Relaxed),
            dropped_events: self.dropped_events.load(Ordering::Relaxed),
            feed_errors: self.feed_errors.load(Ordering::Relaxed),
            input_dropped: self.input_dropped.load(Ordering::Relaxed),
            latency_last: micros(self.latency_last_us.load(Ordering::Relaxed)),
            latency_max: micros(self.latency_max_us.load(Ordering::Relaxed)),
            latency_mean: sum.checked_div(count).map_or(Duration::ZERO, micros),
            latency_samples: count,
        }
    }

    /// Enregistre la latence d'un pas publié. Sans allocation (appelé dans la section temps réel du pas).
    pub(super) fn record_latency(&self, latency: Duration) {
        let us = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        self.latency_last_us.store(us, Ordering::Relaxed);
        self.latency_max_us.fetch_max(us, Ordering::Relaxed);
        self.latency_sum_us.fetch_add(us, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
    }
}

fn micros(us: u64) -> Duration {
    Duration::from_micros(us)
}

/// Instantané des compteurs d'analyse, pour le journal et l'interface opérateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnalysisStatsSnapshot {
    /// Pas publiés depuis le démarrage.
    pub steps: u64,
    /// Événements jetés parce que le canal était plein (lecteur à l'arrêt).
    pub dropped_events: u64,
    /// Flux rejetés ou abandonnés sur erreur (rééchantillonneur impossible, erreur de lecture).
    pub feed_errors: u64,
    /// Échantillons jetés côté callback parce que le ring de la source analysée était plein (cumul du flux courant :
    /// repart de zéro à chaque reconnexion ; mis à jour à chaque bilan du journal).
    pub input_dropped: u64,
    /// Latence callback → publication du dernier pas.
    pub latency_last: Duration,
    /// Plus forte latence mesurée depuis le démarrage.
    pub latency_max: Duration,
    /// Latence moyenne depuis le démarrage.
    pub latency_mean: Duration,
    /// Nombre de pas dont la latence a été mesurée.
    pub latency_samples: u64,
}

/// Fenêtre glissante de latence entre deux bilans du journal (thread d'analyse uniquement).
#[derive(Debug, Default)]
pub(super) struct LatencyWindow {
    count: u32,
    sum: Duration,
    max: Duration,
}

impl LatencyWindow {
    pub(super) fn record(&mut self, latency: Duration) {
        self.count = self.count.saturating_add(1);
        self.sum = self.sum.saturating_add(latency);
        self.max = self.max.max(latency);
    }

    /// Moyenne et maximum de la fenêtre, puis remise à zéro. `None` si aucun pas mesuré.
    pub(super) fn take(&mut self) -> Option<(Duration, Duration, u32)> {
        let window = std::mem::take(self);
        (window.count > 0).then(|| (window.sum / window.count, window.max, window.count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_mean_and_max() {
        let stats = AnalysisStats::default();
        assert_eq!(stats.snapshot().latency_mean, Duration::ZERO);
        stats.record_latency(Duration::from_micros(1_000));
        stats.record_latency(Duration::from_micros(3_000));
        let snap = stats.snapshot();
        assert_eq!(snap.latency_last, Duration::from_micros(3_000));
        assert_eq!(snap.latency_max, Duration::from_micros(3_000));
        assert_eq!(snap.latency_mean, Duration::from_micros(2_000));
        assert_eq!(snap.latency_samples, 2);
    }

    #[test]
    fn window_resets_after_take() {
        let mut window = LatencyWindow::default();
        assert_eq!(window.take(), None);
        window.record(Duration::from_millis(2));
        window.record(Duration::from_millis(4));
        assert_eq!(
            window.take(),
            Some((Duration::from_millis(3), Duration::from_millis(4), 2))
        );
        assert_eq!(window.take(), None);
    }
}
