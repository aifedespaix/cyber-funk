//! Section « Graffitis » de « Réglages monde » : couper ou doser les graffitis muraux et les projecteurs holographiques, et
//! imposer un mot aux projecteurs proches pour juger une police ou un style.
//!
//! Le monde (`cf-world-neon`) lit [`GraffitiTuning`] et re-tire la ville quand elle change ; le panneau ne fait qu'écrire
//! cette ressource, par un geste de l'opérateur (jamais à chaque image). Rien n'est sauvegardé : chaque session repart des
//! densités livrées. L'onglet Debug n'existe qu'en Préparation.

use bevy_egui::egui;
use cf_render::{GraffitiTuning, GraffitiWords, MAX_GRAFFITI_DENSITY};

/// Ce que le panneau garde d'un réglage demandé : densités bornées, et mot imposé seulement s'il fait partie des mots
/// holographiques chargés (un mot inconnu n'afficherait rien). `words` absent : aucun mot imposé possible.
pub(crate) fn sanitize(next: GraffitiTuning, words: Option<&GraffitiWords>) -> GraffitiTuning {
    let mut next = next.clamped();
    if next
        .forced_word
        .as_deref()
        .is_some_and(|word| !words.is_some_and(|w| w.contains(word)))
    {
        next.forced_word = None;
    }
    next
}

/// Le nouveau réglage après un geste, ou `None` s'il ne change rien (la ressource n'est alors pas réveillée).
pub(crate) fn committed(
    current: &GraffitiTuning,
    next: GraffitiTuning,
    words: Option<&GraffitiWords>,
) -> Option<GraffitiTuning> {
    let next = sanitize(next, words);
    (next != *current).then_some(next)
}

/// Dessine la section. Rend le nouveau réglage seulement si l'opérateur y a touché.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    tuning: &GraffitiTuning,
    words: Option<&GraffitiWords>,
) -> Option<GraffitiTuning> {
    let mut next = tuning.clone();
    ui.checkbox(&mut next.wall_enabled, "Graffitis muraux");
    ui.add_enabled(
        next.wall_enabled,
        egui::Slider::new(&mut next.wall_density, 0.0..=MAX_GRAFFITI_DENSITY).text("Densité murale"),
    );
    ui.checkbox(&mut next.holo_enabled, "Projecteurs holographiques");
    ui.add_enabled(
        next.holo_enabled,
        egui::Slider::new(&mut next.holo_density, 0.0..=MAX_GRAFFITI_DENSITY).text("Densité projecteurs"),
    );
    let list = words.map_or(&[][..], |w| w.0.as_slice());
    let shown = next.forced_word.clone().unwrap_or_else(|| "Mots tirés".to_owned());
    egui::ComboBox::from_label("Mot imposé")
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut next.forced_word, None, "Mots tirés");
            for word in list {
                ui.selectable_value(&mut next.forced_word, Some(word.clone()), word);
            }
        });
    if list.is_empty() {
        ui.weak("Aucun mot holographique chargé.");
    }
    committed(tuning, next, words)
}
