//! Les lignes de lancement (« puces ») : un clic lance l'événement, un clic droit l'épingle en tête de sa colonne.
//!
//! Un clic écrit `Action::Trigger(id)`, le chemin du MIDI : le limiteur de flashs reste aux commandes. Pendant que
//! l'événement tourne, la puce se remplit selon son avancement (la plus avancée de ses instances) et affiche `×N` quand
//! il tourne plusieurs fois. Le catalogue ne dit pas quels événements flashent : le point jaune de la maquette n'est pas
//! dessiné (rien à afficher sans inventer).

use bevy_egui::egui;
use cf_control::{Action, TriggerId};
use cf_render::{ActiveEvents, WorldEvent};

use super::columns::runs_as;
use super::rows::{Row, draw_row};

/// Au plus autant d'ordres par image (puces, plans, pad) : une rafale de clics ne remplit jamais la file du chef du
/// spectacle.
pub(crate) const MAX_ORDERS_PER_FRAME: usize = 8;

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

/// Dessine l'événement comme une ligne de la liste : nom (avec `★` et `×N`), note dessous, remplissage d'avancement.
pub(crate) fn draw_chip(
    ui: &mut egui::Ui,
    event: &WorldEvent,
    pinned: bool,
    run: ChipRun,
    color: egui::Color32,
) -> Option<ChipClick> {
    let title = chip_text(event.label, pinned, run);
    let response = draw_row(
        ui,
        &Row {
            detail: event.note,
            progress: run.progress,
            running: run.count > 0,
            ..Row::new(&title, color)
        },
    );
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
