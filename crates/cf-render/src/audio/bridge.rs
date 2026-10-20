//! Le côté lecture du pont : prend le dernier `AudioFeatures` et les événements du thread d'analyse, les passe dans la
//! ligne à retard et rend ce qui doit se voir à l'image. Sans Bevy, donc testable seul.

use cf_analysis::{AnalysisOutputs, AudioEvent};

use super::latency::LatencyLine;

/// Lecteur des sorties de l'analyse, avec son décalage visuel.
#[derive(Debug)]
pub struct AudioBridge {
    outputs: AnalysisOutputs,
    line: LatencyLine,
}

impl AudioBridge {
    /// Pont sur les sorties d'un thread d'analyse.
    #[must_use]
    pub fn new(outputs: AnalysisOutputs) -> Self {
        Self {
            outputs,
            line: LatencyLine::new(),
        }
    }

    /// Une image : lit le dernier état et vide le canal d'événements (sans attendre), puis rend l'état d'il y a
    /// `delay_s` secondes. `on_event` reçoit, dans l'ordre, les événements dont l'heure (plus le décalage) est venue.
    ///
    /// `now_s` est l'horloge murale de l'image (`Time<Real>`). Renvoie `None` tant que l'historique ne remonte pas
    /// assez loin pour le décalage demandé (les `delay_s` premières secondes).
    ///
    /// Aucune allocation en régime établi.
    pub fn poll(
        &mut self,
        now_s: f64,
        delay_s: f64,
        mut on_event: impl FnMut(&AudioEvent),
    ) -> Option<cf_analysis::AudioFeatures> {
        let latest = *self.outputs.features.latest();
        self.line.push_features(now_s, latest);
        while let Ok(event) = self.outputs.events.try_recv() {
            // Même horloge audio des deux côtés : l'écart est l'âge de l'événement à l'instant de la lecture.
            let age_s = (latest.time_s - event.time_s).max(0.0);
            self.line.push_event(now_s - age_s, event);
        }
        while let Some(event) = self.line.pop_due_event(now_s, delay_s) {
            on_event(&event);
        }
        self.line.features_at(now_s, delay_s)
    }

    /// Événements jetés par la ligne à retard faute de place.
    #[must_use]
    pub fn dropped_events(&self) -> u64 {
        self.line.dropped()
    }
}

#[cfg(test)]
mod tests {
    use cf_analysis::{AnalysisConfig, AnalysisThread, AudioEventKind, NeutralAnalyzer};

    use super::*;

    #[test]
    fn un_pont_sans_source_rend_l_etat_neutre_sans_evenement() {
        let config = AnalysisConfig {
            initial_source: None,
            ..AnalysisConfig::default()
        };
        let (_thread, outputs) = AnalysisThread::spawn(NeutralAnalyzer, config).expect("thread d'analyse");
        let mut bridge = AudioBridge::new(outputs);
        let mut events = Vec::new();
        let features = bridge.poll(1.0, 0.0, |e: &AudioEvent| events.push(e.kind));
        assert!(features.is_some_and(|f| !f.live));
        assert!(events.is_empty() || events.iter().all(|k| !matches!(k, AudioEventKind::Kick)));
    }
}
