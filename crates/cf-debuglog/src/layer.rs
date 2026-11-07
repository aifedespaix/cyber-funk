//! Couche `tracing` : les avertissements et erreurs de toutes les bibliothèques (dont `wgpu`, qui annonce les pertes
//! de périphérique graphique) rejoignent le journal structuré.

use std::fmt::Write as _;

use serde_json::json;
use tracing::field::{Field, Visit};
use tracing::{Event, Level as TLevel, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use crate::recorder::{Level, Recorder};

/// À ajouter à l'abonné global : `registry().with(RecorderLayer::new(recorder))`.
#[derive(Debug, Clone)]
pub struct RecorderLayer {
    recorder: Recorder,
}

impl RecorderLayer {
    /// Couche qui écrit dans `recorder`.
    #[must_use]
    pub const fn new(recorder: Recorder) -> Self {
        Self { recorder }
    }
}

#[derive(Default)]
struct Text(String);

impl Visit for Text {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

impl<S: Subscriber> Layer<S> for RecorderLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = match *meta.level() {
            TLevel::ERROR => Level::Error,
            TLevel::WARN => Level::Warn,
            _ => return,
        };
        let mut text = Text::default();
        event.record(&mut text);
        self.recorder.point(
            level,
            "log",
            Some(meta.target()),
            Some(text.0.trim()),
            json!({ "file": meta.file(), "line": meta.line() }),
        );
    }
}
