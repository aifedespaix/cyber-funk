//! La colonne centrale : l'état du show, le grand bouton Démarrer / Arrêter, et les quatre chiffres utiles en direct.

use bevy_egui::egui;
use cf_render::audio::Section;

use crate::format;
use crate::run_state::{ShowRun, ShowRunState};

/// Vert du bouton Démarrer.
const GO_GREEN: egui::Color32 = egui::Color32::from_rgb(24, 140, 80);
/// Rouge du bouton Arrêter.
const STOP_RED: egui::Color32 = egui::Color32::from_rgb(190, 45, 55);
/// Gris du bouton pendant une séquence.
const BUSY_GREY: egui::Color32 = egui::Color32::from_gray(70);

/// Libellé et couleur du grand bouton selon l'état du show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ButtonSpec {
    pub label: &'static str,
    pub fill: egui::Color32,
    pub enabled: bool,
}

/// Le bouton de cet état : vert « Démarrer » en attente, rouge « Arrêter » en cours, grisé pendant une séquence.
pub(crate) fn button_spec(run: ShowRun) -> ButtonSpec {
    match run {
        ShowRun::Waiting => ButtonSpec {
            label: "▶  DÉMARRER",
            fill: GO_GREEN,
            enabled: true,
        },
        ShowRun::Running => ButtonSpec {
            label: "■  ARRÊTER",
            fill: STOP_RED,
            enabled: true,
        },
        ShowRun::Starting => ButtonSpec {
            label: "Démarrage…",
            fill: BUSY_GREY,
            enabled: false,
        },
        ShowRun::Stopping => ButtonSpec {
            label: "Arrêt…",
            fill: BUSY_GREY,
            enabled: false,
        },
    }
}

/// Le titre de la carte d'état.
pub(crate) fn run_title(run: ShowRun, dj: Option<&str>) -> String {
    match (run, dj) {
        (ShowRun::Waiting, _) => "En attente".to_owned(),
        (ShowRun::Starting, _) => "Démarrage…".to_owned(),
        (ShowRun::Running, Some(dj)) => format!("En cours · {dj}"),
        (ShowRun::Running, None) => "En cours".to_owned(),
        (ShowRun::Stopping, _) => "Arrêt…".to_owned(),
    }
}

/// Le sous-titre de la carte d'état.
pub(crate) fn run_subtitle(run: ShowRun, elapsed_s: Option<f64>) -> String {
    match (run, elapsed_s) {
        (ShowRun::Waiting, _) => "Caméra fixe sur la route · prêt à démarrer".to_owned(),
        (ShowRun::Running, Some(elapsed)) => format!("Durée du show {}", format::clock(elapsed)),
        (ShowRun::Starting, _) => "La voiture arrive, le quartier se synchronise".to_owned(),
        (ShowRun::Stopping, _) => "Le quartier se désynchronise, la voiture s'éteint".to_owned(),
        (ShowRun::Running, None) => String::new(),
    }
}

/// Le tempo à afficher, ou un tiret quand il n'y en a pas.
pub(crate) fn bpm_text(bpm: Option<f32>) -> String {
    bpm.map_or_else(|| "—".to_owned(), |bpm| format!("{bpm:.0}"))
}

/// La section du morceau en clair.
pub(crate) fn section_text(section: Section) -> &'static str {
    match section {
        Section::Calm => "Calme",
        Section::Groove => "Groove",
        Section::Peak => "Sommet",
    }
}

/// Les chiffres en direct.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveStats {
    pub bpm: Option<f32>,
    pub section: Section,
    pub fps: Option<f32>,
    /// Volume global, 0..1.
    pub level: f32,
    /// Une source livre du signal.
    pub live: bool,
}

/// Dessine la colonne. Rend vrai quand le bouton principal est pressé.
pub(crate) fn center(
    ui: &mut egui::Ui,
    run: &ShowRunState,
    stats: &LiveStats,
    dj: Option<&str>,
    elapsed_s: Option<f64>,
) -> bool {
    let mut pressed = false;
    let spec = button_spec(run.run);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.weak("ÉTAT DU SHOW");
            ui.label(egui::RichText::new(run_title(run.run, dj)).size(28.0).strong());
            ui.weak(run_subtitle(run.run, elapsed_s));
            ui.add_space(14.0);
            let button = egui::Button::new(
                egui::RichText::new(spec.label)
                    .size(24.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(spec.fill)
            .corner_radius(10.0)
            .min_size(egui::vec2(260.0, 68.0));
            pressed = ui.add_enabled(spec.enabled, button).clicked();
            ui.add_space(8.0);
        });
    });
    ui.add_space(10.0);
    tiles(ui, stats);
    pressed
}

fn tiles(ui: &mut egui::Ui, stats: &LiveStats) {
    let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
    ui.horizontal(|ui| {
        tile(ui, width, "BPM", &bpm_text(stats.bpm));
        tile(ui, width, "SECTION", section_text(stats.section));
    });
    ui.horizontal(|ui| {
        tile(
            ui,
            width,
            "IMAGES / S",
            &stats.fps.map_or_else(|| "—".to_owned(), |fps| format!("{fps:.0}")),
        );
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(width - 18.0);
            ui.set_min_height(76.0);
            ui.weak("NIVEAU AUDIO");
            ui.add_space(6.0);
            let text = if stats.live { "" } else { "pas de signal" };
            ui.add(
                egui::ProgressBar::new(stats.level.clamp(0.0, 1.0))
                    .desired_height(18.0)
                    .text(text),
            );
        });
    });
}

fn tile(ui: &mut egui::Ui, width: f32, label: &str, value: &str) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(width - 18.0);
        ui.set_min_height(76.0);
        ui.weak(label);
        ui.label(egui::RichText::new(value).size(34.0).strong());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_bouton_suit_l_etat_du_show() {
        let waiting = button_spec(ShowRun::Waiting);
        assert_eq!(
            (waiting.label, waiting.fill, waiting.enabled),
            ("▶  DÉMARRER", GO_GREEN, true)
        );
        let running = button_spec(ShowRun::Running);
        assert_eq!(
            (running.label, running.fill, running.enabled),
            ("■  ARRÊTER", STOP_RED, true)
        );
        for run in [ShowRun::Starting, ShowRun::Stopping] {
            assert!(!button_spec(run).enabled, "{run:?}");
        }
    }

    #[test]
    fn le_bouton_n_est_actif_que_si_l_etat_l_autorise() {
        for run in [ShowRun::Waiting, ShowRun::Starting, ShowRun::Running, ShowRun::Stopping] {
            assert_eq!(button_spec(run).enabled, run.button_enabled(), "{run:?}");
        }
    }

    #[test]
    fn le_titre_nomme_le_dj_en_scene() {
        assert_eq!(run_title(ShowRun::Running, Some("Nova")), "En cours · Nova");
        assert_eq!(run_title(ShowRun::Running, None), "En cours");
        assert_eq!(run_title(ShowRun::Waiting, Some("Nova")), "En attente");
    }

    #[test]
    fn un_tempo_absent_s_affiche_par_un_tiret() {
        assert_eq!(bpm_text(None), "—");
        assert_eq!(bpm_text(Some(127.6)), "128");
    }
}
