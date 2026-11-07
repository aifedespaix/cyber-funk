//! Le couloir « En cours » d'une colonne : les événements de sa catégorie, lus dans `ActiveEvents`.
//!
//! Le couloir montre tout ce que le monde a publié dans la catégorie, catalogue ou non : les cascades (`stunt:*`) vont
//! avec les voitures, l'enseigne du DJ (`dj-sign`) avec les panneaux, sans puce pour les lancer. ✕ et « Stop » retirent
//! la ligne du registre ; le monde finit l'événement lui-même (aucun directeur ne sait l'arrêter en route).

use bevy_egui::egui;
use cf_render::{ActiveEvents, ActiveId, EventCategory, EventOrigin};

use super::Gesture;

/// Une ligne du couloir.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LaneRow<'a> {
    /// Entrée du registre.
    pub id: ActiveId,
    /// Nom affiché.
    pub label: &'a str,
    /// Lancé par le monde de lui-même (tag `auto`).
    pub auto: bool,
    /// Avancement 0..=1, `None` si la durée est inconnue.
    pub progress: Option<f32>,
    /// Temps restant (s), `None` si la durée est inconnue.
    pub remaining_s: Option<f64>,
}

/// Les lignes d'une catégorie, dans l'ordre de lancement.
pub(crate) fn lane_rows(
    active: &ActiveEvents,
    category: EventCategory,
    now_s: f64,
) -> impl Iterator<Item = LaneRow<'_>> {
    active.in_category(category).map(move |event| LaneRow {
        id: event.id,
        label: &event.label,
        auto: event.origin == EventOrigin::Auto,
        progress: active.progress(event.id, now_s),
        remaining_s: event
            .duration_s
            .map(|duration| (duration - (now_s - event.started_s)).max(0.0)),
    })
}

/// « 42% · 7 s », ou « en cours » sans durée connue.
pub(crate) fn row_time_text(row: &LaneRow<'_>) -> String {
    match (row.progress, row.remaining_s) {
        (Some(progress), Some(remaining)) => format!("{:.0}% · {remaining:.0} s", progress * 100.0),
        _ => "en cours".to_owned(),
    }
}

/// L'en-tête du couloir : « En cours » et le Stop de la colonne.
pub(crate) fn draw_lane_header(ui: &mut egui::Ui, category: EventCategory, gestures: &mut Vec<Gesture>) {
    ui.horizontal(|ui| {
        ui.small("EN COURS");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Stop")
                .on_hover_text("Retire de la liste tout ce qui tourne dans cette colonne (le monde le finit lui-même).")
                .clicked()
            {
                gestures.push(Gesture::StopColumn(category));
            }
        });
    });
}

/// Les lignes du couloir. `indeterminate` : l'avancement à montrer pour une ligne sans durée (celui de la transition
/// de quartier, connu ailleurs), sinon la barre reste vide avec « durée inconnue ».
pub(crate) fn draw_lane(
    ui: &mut egui::Ui,
    active: Option<&ActiveEvents>,
    category: EventCategory,
    now_s: f64,
    color: egui::Color32,
    indeterminate: Option<f32>,
    gestures: &mut Vec<Gesture>,
) {
    let Some(active) = active else {
        ui.weak("Registre absent (monde absent).");
        return;
    };
    let mut any = false;
    for row in lane_rows(active, category, now_s) {
        any = true;
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(6, 3))
            .show(ui, |ui| {
                if draw_row_line(ui, &row).clicked() {
                    gestures.push(Gesture::Stop(row.id));
                }
                let bar = match row.progress.or(indeterminate) {
                    Some(progress) => egui::ProgressBar::new(progress).desired_height(5.0),
                    None => egui::ProgressBar::new(0.0).text("durée inconnue"),
                };
                ui.add(bar.fill(color.gamma_multiply(0.6)));
            });
    }
    if !any {
        ui.weak("Rien en cours");
    }
}

/// La ligne de titre d'une entrée : ✕ et temps restant posés **d'abord**, à droite, puis le tag `auto` et le nom dans ce
/// qui reste. Un libellé tronqué prend toute la largeur restante : dessiné en premier, il pousserait ✕ hors de la
/// colonne (coupé par le défilement) et l'arrêt deviendrait inaccessible. Rend la réponse de ✕.
pub(crate) fn draw_row_line(ui: &mut egui::Ui, row: &LaneRow<'_>) -> egui::Response {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let stop = ui
            .small_button("✕")
            .on_hover_text("Retire de la liste ; le monde finit l'événement lui-même.");
        ui.weak(row_time_text(row));
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if row.auto {
                ui.small("auto")
                    .on_hover_text("Lancé par le monde de lui-même (orchestrateur).");
            }
            ui.add(egui::Label::new(row.label).truncate());
        });
        stop
    })
    .inner
}
