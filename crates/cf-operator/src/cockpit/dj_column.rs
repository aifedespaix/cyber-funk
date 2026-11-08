//! La colonne de gauche : un DJ par carte, un clic pour passer à lui.
//!
//! La couleur d'une carte est celle du thème du DJ (sa couleur principale, « velours »). Un DJ sans thème propre
//! prend le gris neutre : le thème de l'événement n'est pas dans la feuille de conduite, et inventer une couleur
//! serait pire qu'en montrer aucune.

use bevy_egui::egui;
use cf_render::{SceneColor, ThemeLibrary};
use cf_show::model::Theme;

/// Hauteur d'une carte (points egui).
const CARD_HEIGHT: f32 = 58.0;
/// Côté du logo (points egui).
const LOGO_SIZE: f32 = 40.0;
/// Largeur de la barre de couleur à gauche de la carte.
const COLOR_BAR: f32 = 7.0;

/// Gris des DJ sans thème propre.
const NEUTRAL: egui::Color32 = egui::Color32::from_gray(120);

/// Ce qu'une carte affiche.
#[derive(Debug, Clone)]
pub(crate) struct DjCard {
    pub name: String,
    pub color: egui::Color32,
    pub logo: Option<egui::TextureId>,
}

/// Ce que l'opérateur a fait dans la colonne pendant l'image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DjColumnEvent {
    None,
    /// Clic sur la carte de ce rang : passer à ce DJ.
    Pick(usize),
    /// Clic sur « + Nouveau DJ ».
    New,
}

/// La couleur d'un DJ : la couleur principale de son thème, ou le gris neutre sans thème.
pub(crate) fn theme_color(themes: &ThemeLibrary, theme: Option<&Theme>) -> egui::Color32 {
    let Some(theme) = theme else {
        return NEUTRAL;
    };
    let [r, g, b] = themes.resolve(theme).get(SceneColor::Velvet).to_srgb8();
    egui::Color32::from_rgb(r, g, b)
}

/// Dessine la colonne et rend le geste de l'image.
pub(crate) fn dj_cards(ui: &mut egui::Ui, cards: &[DjCard], active: Option<usize>) -> DjColumnEvent {
    let mut event = DjColumnEvent::None;
    ui.heading("DJ");
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (index, card) in cards.iter().enumerate() {
            let is_active = active == Some(index);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), CARD_HEIGHT), egui::Sense::click());
            paint_card(ui, rect, &response, card, is_active);
            if response.clicked() && !is_active {
                event = DjColumnEvent::Pick(index);
            }
            ui.add_space(4.0);
        }
        if cards.is_empty() {
            ui.weak("Aucun DJ dans cette soirée.");
        }
        ui.add_space(4.0);
        if ui
            .add_sized([ui.available_width(), 34.0], egui::Button::new("+ Nouveau DJ"))
            .on_hover_text("Ouvre l'onglet DJs sur un DJ vierge.")
            .clicked()
        {
            event = DjColumnEvent::New;
        }
    });
    event
}

fn paint_card(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response, card: &DjCard, is_active: bool) {
    let visuals = ui.visuals();
    let fill = if is_active {
        card.color.gamma_multiply(0.30)
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, fill);
    if is_active {
        painter.rect_stroke(rect, 8.0, egui::Stroke::new(2.0, card.color), egui::StrokeKind::Inside);
    }
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(COLOR_BAR, rect.height()));
    painter.rect_filled(bar, 4.0, card.color);

    let mut left = rect.left() + COLOR_BAR + 10.0;
    if let Some(texture) = card.logo {
        let logo = egui::Rect::from_center_size(
            egui::pos2(left + LOGO_SIZE / 2.0, rect.center().y),
            egui::vec2(LOGO_SIZE, LOGO_SIZE),
        );
        let full = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture, logo, full, egui::Color32::WHITE);
        left += LOGO_SIZE + 10.0;
    }
    let text_color = visuals.strong_text_color();
    if is_active {
        painter.text(
            egui::pos2(left, rect.center().y - 9.0),
            egui::Align2::LEFT_CENTER,
            &card.name,
            egui::FontId::proportional(17.0),
            text_color,
        );
        painter.text(
            egui::pos2(left, rect.center().y + 12.0),
            egui::Align2::LEFT_CENTER,
            "● EN SCÈNE",
            egui::FontId::proportional(11.0),
            card.color,
        );
    } else {
        painter.text(
            egui::pos2(left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &card.name,
            egui::FontId::proportional(16.0),
            text_color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_show::model::PresetTheme;

    fn themes() -> ThemeLibrary {
        ThemeLibrary::builtin().expect("thèmes livrés")
    }

    #[test]
    fn la_couleur_d_un_dj_est_celle_de_son_theme() {
        let lib = themes();
        let [r, g, b] = lib.preset(PresetTheme::Gold).get(SceneColor::Velvet).to_srgb8();
        assert_eq!(
            theme_color(&lib, Some(&Theme::Preset(PresetTheme::Gold))),
            egui::Color32::from_rgb(r, g, b)
        );
    }

    #[test]
    fn deux_themes_differents_donnent_deux_couleurs_differentes() {
        let lib = themes();
        let gold = theme_color(&lib, Some(&Theme::Preset(PresetTheme::Gold)));
        let frost = theme_color(&lib, Some(&Theme::Preset(PresetTheme::Frost)));
        assert_ne!(gold, frost);
    }

    #[test]
    fn sans_theme_propre_la_carte_est_grise() {
        assert_eq!(theme_color(&themes(), None), NEUTRAL);
    }
}
