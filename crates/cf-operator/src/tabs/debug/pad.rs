//! Le pad voiture du dock. Le pilote (`cf-world-neon::pilot`) n'a aujourd'hui aucune entrée manuelle : seule
//! l'Ascension (`Action::Ascension`, la même que la touche `A`) se demande. Les autres boutons sont affichés grisés
//! (« pas encore pilotable ») pour montrer la disposition voulue ; aucun raccourci clavier ne leur est lié (les flèches
//! et `E` sont déjà pris par le clavier par défaut : `Go`/`Back`, `RandomEvent`).

use bevy_egui::egui;
use cf_control::Action;

use super::chip::push_order;

/// Infobulle d'une commande sans point d'entrée dans le monde.
pub(crate) const NOT_YET: &str = "pas encore pilotable";

/// Un bouton du pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PadMove {
    /// Tonneau vers la gauche.
    RollLeft,
    /// Monter.
    Up,
    /// Tonneau vers la droite.
    RollRight,
    /// Gauche.
    Left,
    /// Stabiliser.
    Stabilise,
    /// Droite.
    Right,
    /// Salto arrière.
    Backflip,
    /// Descendre.
    Down,
    /// Salto avant.
    Frontflip,
    /// L'Ascension (cascade du final).
    Ascension,
}

impl PadMove {
    /// Les boutons : la grille 3 × 3 de la maquette, ligne par ligne, puis l'Ascension.
    pub(crate) const ALL: [Self; 10] = [
        Self::RollLeft,
        Self::Up,
        Self::RollRight,
        Self::Left,
        Self::Stabilise,
        Self::Right,
        Self::Backflip,
        Self::Down,
        Self::Frontflip,
        Self::Ascension,
    ];

    /// Libellé du bouton.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::RollLeft => "⟲ Tonneau G",
            Self::Up => "⬆ Monter",
            Self::RollRight => "⟳ Tonneau D",
            Self::Left => "⬅ Gauche",
            Self::Stabilise => "⏺ Stabiliser",
            Self::Right => "➡ Droite",
            Self::Backflip => "↺ Backflip",
            Self::Down => "⬇ Descendre",
            Self::Frontflip => "↻ Frontflip",
            Self::Ascension => "⇑ Ascension",
        }
    }

    /// L'ordre existant que le bouton envoie, `None` s'il n'y en a pas encore.
    pub(crate) const fn action(self) -> Option<Action> {
        match self {
            Self::Ascension => Some(Action::Ascension),
            _ => None,
        }
    }
}

/// Le pad. Les ordres passent par la file plafonnée de l'image.
pub(crate) fn draw_pad(ui: &mut egui::Ui, orders: &mut Vec<Action>) {
    egui::Grid::new("debug-car-pad")
        .num_columns(3)
        .spacing(egui::vec2(4.0, 4.0))
        .show(ui, |ui| {
            for (i, pad) in PadMove::ALL.into_iter().enumerate() {
                pad_button(ui, pad, orders);
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
}

fn pad_button(ui: &mut egui::Ui, pad: PadMove, orders: &mut Vec<Action>) {
    let button = egui::Button::new(egui::RichText::new(pad.label()).size(12.0)).min_size(egui::vec2(72.0, 28.0));
    match pad.action() {
        Some(action) => {
            if ui
                .add(button)
                .on_hover_text("Demande l'Ascension : elle part au prochain temps fort (comme la touche A).")
                .clicked()
            {
                push_order(orders, action);
            }
        }
        None => {
            ui.add_enabled(false, button).on_disabled_hover_text(NOT_YET);
        }
    }
}
