//! Éléments d'interface communs de la vue Live : style lisible dans le noir, boutons de commande, barres.
//!
//! Principes (`docs/tasks/J8.md`) : contraste fort, grandes cibles, aucun texte sous 14 px.

use bevy_egui::egui::{self, Color32, RichText};
use cf_control::Action;

use super::commands::{CommandContext, LiveCommand};

/// Plus petite taille de texte de la vue Live, en points egui (≈ 14 px) : jamais en dessous.
pub(crate) const MIN_TEXT: f32 = 14.0;
/// Hauteur des gros boutons de conduite (GO, BACK, HOLD).
pub(crate) const BIG_BUTTON_HEIGHT: f32 = 64.0;
/// Hauteur des boutons ordinaires : cible confortable à la souris comme au doigt.
pub(crate) const BUTTON_HEIGHT: f32 = 40.0;

/// Fond d'un bouton dont la bascule est active (noir demandé, hold…) : rouge franc, texte blanc (contraste > 7:1).
pub(crate) const ACTIVE_FILL: Color32 = Color32::from_rgb(170, 20, 40);
/// Fond du cue à l'écran dans la liste.
pub(crate) const CURRENT_FILL: Color32 = Color32::from_rgb(20, 90, 60);
/// Texte d'un cue déjà joué.
pub(crate) const PLAYED_TEXT: Color32 = Color32::from_rgb(150, 156, 170);
/// Texte d'avertissement.
pub(crate) const WARN_TEXT: Color32 = Color32::from_rgb(255, 190, 60);

/// Texte plus gros pour la vue Live, plancher à [`MIN_TEXT`] ; boutons hauts.
pub(crate) fn apply_style(style: &mut egui::Style) {
    use egui::TextStyle::{Body, Button, Heading, Monospace, Small};
    let sizes = [
        (Small, MIN_TEXT),
        (Body, 16.0),
        (Button, 17.0),
        (Monospace, 15.0),
        (Heading, 24.0),
    ];
    for (text_style, size) in sizes {
        if let Some(font) = style.text_styles.get_mut(&text_style) {
            font.size = size;
        }
    }
    // Aucun style, même ajouté plus tard, ne passe sous le plancher.
    for font in style.text_styles.values_mut() {
        font.size = font.size.max(MIN_TEXT);
    }
    style.spacing.interact_size.y = BUTTON_HEIGHT;
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(235, 238, 245);
}

/// Un bouton de commande : libellé et raccourci, taille donnée. `active` : bascule allumée (fond rouge).
/// Rend vrai au clic ; l'appelant décide de l'effet (ordre ou demande de mode).
pub(crate) fn command_button(
    ui: &mut egui::Ui,
    command: LiveCommand,
    size: egui::Vec2,
    active: bool,
    enabled: bool,
) -> bool {
    let text = format!("{}  [{}]", command.label(), command.shortcut_text());
    let mut button = egui::Button::new(RichText::new(text).strong());
    if active {
        button = button.fill(ACTIVE_FILL);
    }
    ui.add_enabled(enabled, button.min_size(size)).clicked()
}

/// Bouton de commande qui produit son `Action` dans `orders` au clic. Rend vrai au clic.
pub(crate) fn action_button(
    ui: &mut egui::Ui,
    command: LiveCommand,
    ctx: &CommandContext<'_>,
    orders: &mut Vec<Action>,
    size: egui::Vec2,
    active: bool,
) -> bool {
    let clicked = command_button(ui, command, size, active, true);
    if clicked {
        orders.extend(command.action(ctx));
    }
    clicked
}

/// Barre de niveau 0..1 avec son étiquette, sur une ligne.
pub(crate) fn level_bar(ui: &mut egui::Ui, label: &str, value: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([72.0, 20.0], egui::Label::new(label));
        ui.add(
            egui::ProgressBar::new(value.clamp(0.0, 1.0))
                .desired_width(ui.available_width().max(80.0))
                .corner_radius(2),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aucun_texte_sous_14_px() {
        let mut style = egui::Style::default();
        // Un style ajouté par un autre module, plus petit que le plancher.
        style.text_styles.insert(
            egui::TextStyle::Name("minuscule".into()),
            egui::FontId::proportional(9.0),
        );
        apply_style(&mut style);
        assert!(!style.text_styles.is_empty());
        for (name, font) in &style.text_styles {
            assert!(font.size >= MIN_TEXT, "{name:?} : {} < {MIN_TEXT}", font.size);
        }
    }
}
