//! Événements ponctuels horodatés en temps audio, poussés dans un canal borné sans jamais bloquer.

use std::sync::atomic::{AtomicU64, Ordering};

use cf_audio::source::SourceKind;
use crossbeam_channel::Sender;

use crate::features::Section;

/// Nature d'un événement d'analyse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEventKind {
    /// Un kick (J1-04).
    Kick,
    /// Changement de section (J1-05).
    SectionChanged {
        /// Nouvelle section.
        section: Section,
    },
    /// L'analyse écoute une autre source (`None` : aucune).
    SourceChanged {
        /// Source désormais analysée.
        source: Option<SourceKind>,
    },
    /// Le signal apparaît ou disparaît (perte de périphérique, pause, reprise).
    LiveChanged {
        /// Nouvel état de `AudioFeatures::live`.
        live: bool,
    },
}

/// Un événement daté.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioEvent {
    /// Temps audio de l'événement (s), même horloge que `AudioFeatures::time_s`. Le pont Bevy (J1-06) le convertit
    /// en temps d'image en appliquant le décalage de latence du projecteur.
    pub time_s: f64,
    /// Pas d'analyse pendant lequel l'événement a été émis.
    pub step: u64,
    /// Nature de l'événement.
    pub kind: AudioEventKind,
}

/// Émetteur d'événements prêté à l'analyseur pendant un pas : `try_send` sur un canal borné, jamais bloquant. Si le
/// canal est plein (lecteur à l'arrêt), l'événement est jeté et compté.
#[derive(Debug)]
pub struct EventSink<'a> {
    tx: &'a Sender<AudioEvent>,
    dropped: &'a AtomicU64,
    step: u64,
    time_s: f64,
}

impl<'a> EventSink<'a> {
    /// Émetteur pour le pas `step`, qui se termine au temps audio `time_s`.
    #[must_use]
    pub fn new(tx: &'a Sender<AudioEvent>, dropped: &'a AtomicU64, step: u64, time_s: f64) -> Self {
        Self {
            tx,
            dropped,
            step,
            time_s,
        }
    }

    /// Émet un événement daté de la fin du pas.
    pub fn emit(&mut self, kind: AudioEventKind) {
        self.emit_at(self.time_s, kind);
    }

    /// Émet un événement daté précisément (par exemple l'échantillon d'attaque d'un kick, à l'intérieur du pas).
    pub fn emit_at(&mut self, time_s: f64, kind: AudioEventKind) {
        let event = AudioEvent {
            time_s,
            step: self.step,
            kind,
        };
        if self.tx.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_channel_drops_and_counts() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let dropped = AtomicU64::new(0);
        let mut sink = EventSink::new(&tx, &dropped, 7, 1.5);
        sink.emit(AudioEventKind::Kick);
        sink.emit_at(1.25, AudioEventKind::LiveChanged { live: true });
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        let event = rx.try_recv().unwrap();
        assert_eq!(event.step, 7);
        assert!((event.time_s - 1.5).abs() < f64::EPSILON);
        assert_eq!(event.kind, AudioEventKind::Kick);
    }
}
