//! Les quatre tuiles du haut : images/s (avec p99), tempo et confiance, événements joués, limiteur de flashs. Une
//! valeur indisponible (plugin absent, tempo pas encore établi) s'écrit « — », jamais 0.

use bevy_egui::egui::{self, RichText};
use cf_render::DiagnosticsSnapshot;
use cf_render::audio::AudioFeatures;

/// Budget d'une image à 60 i/s, en ms : au-delà, le p99 est signalé.
const FRAME_BUDGET_MS: f32 = 1000.0 / 60.0;
/// Taille du chiffre principal d'une tuile (points egui).
const VALUE_SIZE: f32 = 24.0;
/// Valeur absente.
pub(crate) const MISSING: &str = "—";

/// Ce que les tuiles affichent.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TileData<'a> {
    pub diagnostics: Option<&'a DiagnosticsSnapshot>,
    pub features: Option<&'a AudioFeatures>,
    /// Événements lancés depuis l'ouverture, si le registre des événements existe.
    pub events_played: Option<u32>,
    /// Flashs permis par le limiteur (`None` : post-process absent).
    pub flashes_permitted: Option<bool>,
}

/// Les quatre tuiles, en colonnes égales.
pub(crate) fn draw(ui: &mut egui::Ui, data: &TileData<'_>) {
    ui.columns(4, |cols| {
        let (fps, fps_sub) = data.diagnostics.map_or_else(
            || (MISSING.to_owned(), "diagnostic absent".to_owned()),
            |d| {
                let over = if d.frame_p99_ms > FRAME_BUDGET_MS {
                    " · hors budget"
                } else {
                    ""
                };
                (format!("{:.0}", d.fps), format!("p99 {:.1} ms{over}", d.frame_p99_ms))
            },
        );
        tile(&mut cols[0], "Images / s", &fps, &fps_sub);

        let (bpm, bpm_sub) = match data.features {
            Some(f) => (
                f.bpm.map_or_else(|| MISSING.to_owned(), |bpm| format!("{bpm:.1}")),
                format!("confiance {:.0} %", f.confidence.clamp(0.0, 1.0) * 100.0),
            ),
            None => (MISSING.to_owned(), "audio absent".to_owned()),
        };
        tile(&mut cols[1], "Tempo détecté", &bpm, &bpm_sub);

        let played = data.events_played.map_or_else(|| MISSING.to_owned(), |n| n.to_string());
        tile(&mut cols[2], "Événements joués", &played, "depuis l'ouverture");

        let (limiter, limiter_sub) = match data.flashes_permitted {
            Some(true) => ("OK", "flashs permis (WCAG 2.3.1)"),
            Some(false) => ("Coupés", "coupure des flashs ou respiration"),
            None => (MISSING, "post-process absent"),
        };
        tile(&mut cols[3], "Limiteur de flashs", limiter, limiter_sub);
    });
}

fn tile(ui: &mut egui::Ui, title: &str, value: &str, sub: &str) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.weak(title);
        ui.label(RichText::new(value).size(VALUE_SIZE).strong());
        ui.small(sub);
    });
}
