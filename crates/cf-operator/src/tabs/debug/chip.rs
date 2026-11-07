//! Les puces de lancement : un clic lance l'événement, un clic droit l'épingle en tête de sa colonne.
//!
//! Un clic écrit `Action::Trigger(id)`, le chemin du MIDI : le limiteur de flashs reste aux commandes. Pendant que
//! l'événement tourne, la puce se remplit selon son avancement (la plus avancée de ses instances) et affiche `×N` quand
//! il tourne plusieurs fois. Le catalogue ne dit pas quels événements flashent : le point jaune de la maquette n'est pas
//! dessiné (rien à afficher sans inventer).

use bevy_egui::egui;
use cf_control::{Action, TriggerId};
use cf_render::{ActiveEvents, WorldEvent};

use super::columns::runs_as;

/// Au plus autant d'ordres par image (puces, plans, pad) : une rafale de clics ne remplit jamais la file du chef du
/// spectacle.
pub(crate) const MAX_ORDERS_PER_FRAME: usize = 8;

/// Hauteur minimale d'une puce (px) : cible cliquable d'au moins 24 px.
const CHIP_HEIGHT: f32 = 24.0;

/// Ajoute un ordre. `false` quand la file de l'image est pleine : le clic est perdu sans bruit.
pub(crate) fn push_order(orders: &mut Vec<Action>, action: Action) -> bool {
    if orders.len() >= MAX_ORDERS_PER_FRAME {
        return false;
    }
    orders.push(action);
    true
}

/// Écrit l'ordre de lancer cet événement (voir [`push_order`]).
pub(crate) fn fire(orders: &mut Vec<Action>, event: &WorldEvent) -> bool {
    push_order(orders, Action::Trigger(TriggerId::new(event.id)))
}

/// Ce qui tourne d'une puce.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ChipRun {
    /// Instances en cours.
    pub count: usize,
    /// Avancement de la plus avancée, 0..=1 ; `None` si rien ne tourne ou si aucune n'a de durée connue.
    pub progress: Option<f32>,
}

/// Les instances en cours de la puce `chip_id` (correspondance de [`runs_as`]).
pub(crate) fn chip_run(chip_id: &str, active: &ActiveEvents, now_s: f64) -> ChipRun {
    active
        .iter()
        .filter(|event| runs_as(chip_id, event.event_id))
        .fold(ChipRun::default(), |run, event| ChipRun {
            count: run.count + 1,
            progress: match (run.progress, active.progress(event.id, now_s)) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
        })
}

/// Le texte de la puce : `★` si épinglée, `×N` si plusieurs instances tournent.
pub(crate) fn chip_text(label: &str, pinned: bool, run: ChipRun) -> String {
    let star = if pinned { "★ " } else { "" };
    if run.count > 1 {
        format!("{star}{label} ×{}", run.count)
    } else {
        format!("{star}{label}")
    }
}

/// Ce que l'opérateur a fait d'une puce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChipClick {
    /// Clic : lancer.
    Fire,
    /// Clic droit : épingler ou désépingler.
    TogglePin,
}

/// Dessine une puce. Le fond (et son remplissage) est peint sous le bouton, laissé transparent.
pub(crate) fn draw_chip(
    ui: &mut egui::Ui,
    event: &WorldEvent,
    pinned: bool,
    run: ChipRun,
    color: egui::Color32,
) -> Option<ChipClick> {
    // Deux places réservées sous le bouton (fond, puis remplissage) : rien à allouer par image.
    let background = ui.painter().add(egui::Shape::Noop);
    let fill = ui.painter().add(egui::Shape::Noop);
    let text = egui::RichText::new(chip_text(event.label, pinned, run)).size(12.0);
    let stroke = if run.count > 0 {
        egui::Stroke::new(1.0, color)
    } else {
        ui.visuals().widgets.inactive.bg_stroke
    };
    let response = ui.add(
        egui::Button::new(text)
            .fill(egui::Color32::TRANSPARENT)
            .stroke(stroke)
            .min_size(egui::vec2(0.0, CHIP_HEIGHT)),
    );
    let rect = response.rect;
    let radius = ui.visuals().widgets.inactive.corner_radius;
    let base = ui.visuals().widgets.inactive.weak_bg_fill;
    ui.painter()
        .set(background, egui::Shape::rect_filled(rect, radius, base));
    if let Some(progress) = run.progress {
        let mut filled = rect;
        filled.set_width(rect.width() * progress.clamp(0.0, 1.0));
        ui.painter().set(
            fill,
            egui::Shape::rect_filled(filled, radius, color.gamma_multiply(0.35)),
        );
    }
    let response = response.on_hover_ui(|ui| {
        if !event.note.is_empty() {
            ui.label(event.note);
        }
        ui.weak("Clic : lancer (le limiteur de flashs reste actif) · clic droit : épingler");
    });
    if response.secondary_clicked() {
        Some(ChipClick::TogglePin)
    } else if response.clicked() {
        Some(ChipClick::Fire)
    } else {
        None
    }
}
