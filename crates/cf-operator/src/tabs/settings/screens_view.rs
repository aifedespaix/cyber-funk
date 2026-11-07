//! La carte « Écran de sortie du rendu » de l'onglet Réglages : la liste des écrans (numéro, nom, résolution,
//! fréquence), l'écran du projecteur en surbrillance, le plein écran et l'identification des écrans.
//!
//! La carte ne fait qu'émettre des [`ScreenOrder`] (voir `services::screens`) : le binaire déplace la fenêtre, un
//! système de l'opérateur retient le choix. « Afficher le numéro sur chaque écran » demanderait une fenêtre par écran :
//! le bouton est là, grisé, « pas encore disponible ».

use bevy_egui::egui;
use cf_render::{ScreenInfo, ScreenList, ScreenOrder};

use crate::services::screens::{ScreenCardState, armed_slot, pick};

/// Hauteur minimale d'une ligne cliquable (cibles d'au moins 24 px).
const ROW_HEIGHT: f32 = 28.0;

/// Ce que la carte lit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScreenCard<'a> {
    /// Écrans détectés (vide sans projecteur branché, ou avant la première image).
    pub list: &'a ScreenList,
    /// Plein écran retenu.
    pub fullscreen: bool,
    /// Mode Live : changer d'écran demande deux clics.
    pub live: bool,
    /// Secondes réelles de l'application (confirmation).
    pub now_s: f64,
}

/// Une ligne : « 2  HDMI-1  1920 × 1080 · 60 Hz · principal ».
fn row_text(number: usize, screen: &ScreenInfo) -> String {
    let name = screen.name.as_deref().unwrap_or("(écran sans nom)");
    // Arrondi au hertz : 59 940 mHz s'affiche 60 Hz, comme dans les réglages du système.
    let rate = screen
        .refresh_mhz
        .map(|mhz| format!(" · {} Hz", (mhz + 500) / 1000))
        .unwrap_or_default();
    let primary = if screen.primary { " · principal" } else { "" };
    let projector = if screen.projector { " · projecteur" } else { "" };
    format!(
        "{number}  {name}  {} × {}{rate}{primary}{projector}",
        screen.width, screen.height
    )
}

/// Dessine la carte ; les ordres de l'image vont dans `orders`.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    card: ScreenCard<'_>,
    state: &mut ScreenCardState,
    orders: &mut Vec<ScreenOrder>,
) {
    ui.heading("Écran de sortie du rendu");
    let screens = &card.list.0;
    if screens.is_empty() {
        ui.weak("Aucun écran détecté pour l'instant.");
    }
    let armed = armed_slot(card.list, card.now_s, state);
    for (slot, screen) in screens.iter().enumerate() {
        let text = if armed == Some(slot) {
            format!("Confirmer : projeter sur {}", screen.name.as_deref().unwrap_or("?"))
        } else {
            row_text(slot + 1, screen)
        };
        let button =
            egui::Button::selectable(screen.projector, text).min_size(egui::vec2(ui.available_width(), ROW_HEIGHT));
        let response = ui.add_enabled(screen.name.is_some(), button);
        let response = if screen.name.is_some() {
            response.on_hover_text(format!("Numéro de --projector-monitor : {slot}"))
        } else {
            response.on_disabled_hover_text(format!(
                "Écran sans nom : impossible à retenir, choisir avec --projector-monitor {slot}"
            ))
        };
        if response.clicked()
            && let Some(order) = pick(card.list, slot, card.live, card.now_s, state)
        {
            orders.push(order);
        }
    }
    if armed.is_some() {
        ui.colored_label(
            egui::Color32::YELLOW,
            "Live : cliquer à nouveau pour déplacer la projection.",
        );
    }
    if screens.len() == 1 {
        ui.weak("Un seul écran détecté");
    }
    ui.separator();
    let mut fullscreen = card.fullscreen;
    if ui.checkbox(&mut fullscreen, "Plein écran").changed() {
        orders.push(ScreenOrder::SetFullscreen(fullscreen));
    }
    ui.add_enabled(false, egui::Button::new("Afficher le numéro sur chaque écran (3 s)"))
        .on_disabled_hover_text("pas encore disponible");
    ui.weak(
        "Le changement est appliqué tout de suite. La fenêtre opérateur reste où elle est. Mémorisé pour le \
         prochain lancement (--projector-monitor garde la priorité).",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screens(count: usize) -> ScreenList {
        ScreenList(
            (0..count)
                .map(|i| ScreenInfo {
                    name: (i != 2).then(|| format!("Écran {i}")),
                    width: 1920,
                    height: 1080,
                    refresh_mhz: Some(59_940),
                    primary: i == 0,
                    projector: i == 0,
                })
                .collect(),
        )
    }

    fn draw_at(width: f32, height: f32, count: usize, live: bool) -> Vec<ScreenOrder> {
        let list = screens(count);
        let ctx = egui::Context::default();
        let mut state = ScreenCardState::default();
        let mut orders = Vec::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                ..egui::RawInput::default()
            };
            ctx.run_ui(raw, |ui| {
                let card = ScreenCard {
                    list: &list,
                    fullscreen: true,
                    live,
                    now_s: 1.0,
                };
                draw(ui, card, &mut state, &mut orders);
            })
            .drop_without_applying_deltas();
        }
        orders
    }

    #[test]
    fn the_card_draws_with_0_1_and_3_screens_at_two_sizes() {
        for (width, height) in [(640.0, 400.0), (1280.0, 720.0)] {
            for count in [0, 1, 3] {
                for live in [false, true] {
                    assert!(
                        draw_at(width, height, count, live).is_empty(),
                        "aucun clic, aucun ordre"
                    );
                }
            }
        }
    }

    #[test]
    fn a_row_shows_number_name_resolution_and_rate() {
        let list = screens(1);
        assert_eq!(
            row_text(1, &list.0[0]),
            "1  Écran 0  1920 × 1080 · 60 Hz · principal · projecteur"
        );
    }
}
