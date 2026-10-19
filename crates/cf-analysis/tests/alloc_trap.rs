//! Preuve que la garde « aucune allocation par pas » du thread d'analyse est armée : sans ce test, les autres tests
//! d'intégration (qui comptent sur un avortement en cas d'allocation) pourraient passer pour de mauvaises raisons.
//!
//! Le test se relance lui-même dans un processus enfant, avec un analyseur qui alloue à chaque pas. L'enfant doit
//! avorter (« memory allocation of … failed ») ; s'il survit, la garde ne protège rien et le test échoue.

use std::time::{Duration, Instant};

use cf_analysis::{AnalysisConfig, AnalysisThread, Analyzer, AudioFeatures, EventSink, StepContext};
use cf_audio::feed::{HOP, feed};
use cf_audio::source::SourceKind;

// `AllocDisabler` n'existe qu'en debug (en release, `assert_no_alloc` est compilé en appel direct).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const CHILD_ENV: &str = "CF_ANALYSIS_ALLOC_TRAP_CHILD";

/// Analyseur fautif : alloue un tampon à chaque pas.
#[derive(Debug)]
struct Allocating;

impl Analyzer for Allocating {
    fn step(&mut self, hop: &[f32; HOP], _: &StepContext, out: &mut AudioFeatures, _: &mut EventSink<'_>) {
        let copy = std::hint::black_box(hop.to_vec());
        out.loud = copy.first().copied().unwrap_or_default();
    }

    fn reset(&mut self) {}
}

fn run_child() -> Result<(), Box<dyn std::error::Error>> {
    let config = AnalysisConfig {
        initial_source: Some(SourceKind::Demo),
        ..AnalysisConfig::default()
    };
    let (analysis, _outputs) = AnalysisThread::spawn(Allocating, config)?;
    let (mut writer, input) = feed(SourceKind::Demo, 48_000, 4 * HOP)?;
    analysis.control().attach(input);
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        writer.push_interleaved(&[0.5_f32; HOP], 1);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Arrivé ici, l'analyseur a alloué sans que rien ne l'arrête : l'enfant sort normalement, le parent échouera.
    Ok(())
}

#[cfg(debug_assertions)]
#[test]
fn allocating_analyzer_aborts_the_process() {
    if std::env::var_os(CHILD_ENV).is_some() {
        run_child().unwrap();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "allocating_analyzer_aborts_the_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "l'enfant aurait dû avorter : {stderr}");
    assert!(
        stderr.contains("memory allocation of"),
        "avortement inattendu : {stderr}"
    );
}
