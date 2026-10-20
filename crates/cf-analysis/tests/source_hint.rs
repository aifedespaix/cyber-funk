//! Le thread d'analyse annonce à l'analyseur quelle source il écoute (`Analyzer::set_source`) : au démarrage et à
//! chaque changement. C'est ce qui active le gain automatique du micro (capture seulement).

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cf_analysis::{AnalysisConfig, AnalysisThread, Analyzer, AudioFeatures, EventSink, StepContext};
use cf_audio::feed::HOP;
use cf_audio::source::SourceKind;

/// Code de la dernière source annoncée : 0 aucune, 1 capture, 2 démo, 255 jamais annoncée.
struct Spy(Arc<AtomicU8>);

impl Analyzer for Spy {
    fn step(&mut self, _: &[f32; HOP], _: &StepContext, _: &mut AudioFeatures, _: &mut EventSink<'_>) {}

    fn set_source(&mut self, source: Option<SourceKind>) {
        let code = match source {
            None => 0,
            Some(SourceKind::Capture) => 1,
            Some(SourceKind::Demo) => 2,
        };
        self.0.store(code, Ordering::SeqCst);
    }

    fn reset(&mut self) {}
}

fn wait_for_code(seen: &AtomicU8, expected: u8) {
    let start = Instant::now();
    while seen.load(Ordering::SeqCst) != expected {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "source annoncée {} au lieu de {expected}",
            seen.load(Ordering::SeqCst)
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn analysis_thread_announces_the_selected_source() {
    let seen = Arc::new(AtomicU8::new(255));
    let config = AnalysisConfig {
        initial_source: Some(SourceKind::Demo),
        ..AnalysisConfig::default()
    };
    let (analysis, _outputs) = AnalysisThread::spawn(Spy(Arc::clone(&seen)), config).expect("thread d'analyse");
    wait_for_code(&seen, 2);
    analysis.control().select(Some(SourceKind::Capture));
    wait_for_code(&seen, 1);
    analysis.control().select(None);
    wait_for_code(&seen, 0);
}
