//! Écran de progression du chargement (J4-07) : ce que l'opérateur voit pendant que le projecteur reste noir.
//!
//! Les étapes de préchargement (voiture, musiques, logos), puis la compilation des pipelines. Une étape en échec est
//! montrée en rouge avec sa raison : un asset manquant se découvre ici, à l'ouverture, pas pendant un cue.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use cf_render::warmup::{LoadStep, PipelineWatch, STABLE_FRAMES, StepStatus, WarmupPhase};
use cf_render::{AppState, LatePipelines, LoadTracker, WarmupProgress};

/// Tout ce que l'écran lit. Chaque ressource est optionnelle : sans `WarmupPlugin`, l'application est « prête ».
#[derive(SystemParam)]
pub(crate) struct LoadingView<'w> {
    state: Option<Res<'w, State<AppState>>>,
    tracker: Option<Res<'w, LoadTracker>>,
    progress: Option<Res<'w, WarmupProgress>>,
    watch: Option<Res<'w, PipelineWatch>>,
    late: Option<Res<'w, LatePipelines>>,
}

impl LoadingView<'_> {
    /// Vrai pendant le chargement.
    pub(crate) fn is_loading(&self) -> bool {
        self.state.as_ref().is_some_and(|state| *state.get() == AppState::Loading)
    }

    /// Compilations tardives constatées depuis `Ready` (0 est l'état sain).
    pub(crate) fn late_pipelines(&self) -> usize {
        self.late.as_ref().map_or(0, |late| late.0)
    }
}

/// Phrase décrivant ce que fait le chargement.
fn phase_label(phase: WarmupPhase, waiting: usize) -> String {
    match phase {
        WarmupPhase::Assets => "Chargement des fichiers…".to_owned(),
        WarmupPhase::Compiling if waiting > 0 => format!("Compilation des shaders… ({waiting} en attente)"),
        WarmupPhase::Compiling => "Vérification de la compilation…".to_owned(),
        WarmupPhase::Done => "Prêt.".to_owned(),
    }
}

/// Part du chargement accomplie, dans `[0, 1]` : les fichiers pour moitié, la compilation pour l'autre.
fn overall_fraction(files: f32, phase: WarmupPhase, stable_fraction: f32) -> f32 {
    match phase {
        WarmupPhase::Assets => files * 0.5,
        WarmupPhase::Compiling => 0.5 + stable_fraction.clamp(0.0, 1.0) * 0.5,
        WarmupPhase::Done => 1.0,
    }
}

/// Dessine l'écran de chargement.
pub(crate) fn draw(ui: &mut egui::Ui, view: &LoadingView) {
    ui.heading("Chargement du show");
    let (Some(tracker), Some(progress)) = (view.tracker.as_deref(), view.progress.as_deref()) else {
        ui.weak("Préchargement non branché (WarmupPlugin absent).");
        return;
    };
    let waiting = view.watch.as_deref().map_or(0, PipelineWatch::waiting);
    // Des images calmes de suite finissent la compilation (voir `warmup::plugin`) : la barre suit cette échéance.
    let stable = progress.stable_frames as f32 / STABLE_FRAMES as f32;
    let fraction = overall_fraction(tracker.fraction(), progress.phase, stable);
    ui.add(
        egui::ProgressBar::new(fraction)
            .show_percentage()
            .text(phase_label(progress.phase, waiting)),
    );
    ui.label(format!("{:.0} s écoulées", progress.elapsed_s));
    ui.separator();
    for step in tracker.steps() {
        draw_step(ui, step);
    }
    if !progress.entries.is_empty() {
        ui.separator();
        ui.label("Préchauffe des shaders");
        for (label, copies) in &progress.entries {
            ui.weak(format!("{label} : {copies} variante(s)"));
        }
    }
}

fn draw_step(ui: &mut egui::Ui, step: &LoadStep) {
    match &step.status {
        StepStatus::Pending => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&step.label);
            });
        }
        StepStatus::Done => {
            ui.label(format!("OK  {}", step.label));
        }
        StepStatus::Failed(reason) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("ÉCHEC  {} : {reason}", step.label));
        }
    }
}

/// Bandeau de l'état sain ou du défaut après `Ready` : le détecteur de régression, côté opérateur.
pub(crate) fn draw_late_warning(ui: &mut egui::Ui, late: usize) {
    if late > 0 {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            format!("{late} pipeline(s) compilé(s) pendant le show : voir le journal (« pipeline compilé après Ready »)."),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_barre_avance_sans_reculer_d_une_phase_a_l_autre() {
        let a = overall_fraction(1.0, WarmupPhase::Assets, 0.0);
        let b = overall_fraction(1.0, WarmupPhase::Compiling, 0.0);
        let c = overall_fraction(1.0, WarmupPhase::Compiling, 1.0);
        let d = overall_fraction(1.0, WarmupPhase::Done, 0.0);
        assert!(a <= b && b <= c && c <= d, "{a} {b} {c} {d}");
        assert!((d - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn la_phrase_dit_ce_qui_attend() {
        assert!(phase_label(WarmupPhase::Compiling, 3).contains("3 en attente"));
        assert!(phase_label(WarmupPhase::Assets, 0).contains("fichiers"));
    }
}
