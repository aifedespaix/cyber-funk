//! La section « Météo » de « Réglages monde » : forcer ou couper la pluie, et allumer ou éteindre la brume, le brouillard
//! bas et les éclairs. Le dessin lit les réglages et ne rend le nouveau que s'il change : la ressource du monde n'est
//! écrite que sur un vrai geste.

use bevy_egui::egui;
use cf_render::{RainMode, WeatherControl};

/// Le dessin. Rend les nouveaux réglages seulement s'ils diffèrent des actuels.
pub(crate) fn draw(ui: &mut egui::Ui, current: WeatherControl) -> Option<WeatherControl> {
    let mut next = current;
    ui.small("PLUIE");
    ui.horizontal(|ui| {
        for mode in RainMode::ALL {
            if ui
                .selectable_label(next.rain == mode, mode.label())
                .on_hover_text(rain_hint(mode))
                .clicked()
            {
                next.rain = mode;
            }
        }
    });
    ui.small("AMBIANCE");
    ui.checkbox(&mut next.mist, "Brume sous la route");
    ui.checkbox(&mut next.haze, "Brouillard bas")
        .on_hover_text("Les voiles qui rampent à ras de la chaussée.");
    ui.checkbox(&mut next.lightning, "Éclairs d'orage")
        .on_hover_text("Toujours derrière le limiteur photosensible : cocher ne le contourne pas.");
    if ui
        .add_enabled(!next.is_default(), egui::Button::new("Tout en automatique"))
        .clicked()
    {
        next = WeatherControl::default();
    }
    (next != current).then_some(next)
}

/// Infobulle d'un mode de pluie.
pub(crate) const fn rain_hint(mode: RainMode) -> &'static str {
    match mode {
        RainMode::Auto => "Les averses suivent le niveau du spectacle (jamais en pré-intro).",
        RainMode::Forced => "Il pleut tout de suite et tant que ce mode dure, même sans spectacle ni musique.",
        RainMode::Off => "Pas une goutte : l'averse en cours s'achève et aucune ne vient.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_once(current: WeatherControl) -> Option<WeatherControl> {
        let ctx = egui::Context::default();
        let mut out = None;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0))),
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(raw, |ui| out = draw(ui, current));
        // Les textures de la première image ne sont pas appliquées : on les jette exprès.
        output.textures_delta.clear();
        out
    }

    #[test]
    fn une_image_sans_clic_ne_change_rien() {
        assert_eq!(run_once(WeatherControl::default()), None);
        let custom = WeatherControl {
            rain: RainMode::Forced,
            mist: false,
            ..WeatherControl::default()
        };
        assert_eq!(run_once(custom), None);
    }

    #[test]
    fn chaque_mode_de_pluie_a_une_infobulle() {
        for mode in RainMode::ALL {
            assert!(!rain_hint(mode).is_empty());
        }
    }
}
