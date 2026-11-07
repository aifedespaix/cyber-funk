//! Le dock à droite de la grille : plans caméra, pad voiture, « Autres » (familles sans colonne) et « Réglages monde »
//! (façades et choix de la voiture, repris de l'ancienne fenêtre « Debug monde »).
//!
//! Plans caméra : le réalisateur (`cf-cinema`) n'offre aujourd'hui que deux entrées, « Plan suivant »
//! (`Action::SkipShot`, touche `N`) et « Enseigne DJ » (`Action::ShowDj`, touche `T`). Choisir un plan précis ou sa
//! transition d'entrée n'a pas de point d'entrée : l'interface le dit au lieu d'en inventer un.

use bevy_egui::egui;
use cf_control::Action;
use cf_render::{CarSelection, FacadeTuning};
use cf_show::persist::{AppSettings, CarChoice};

use super::chip::{ChipClick, chip_run, draw_chip, fire, push_order};
use super::pad::draw_pad;
use super::persist::{family_is_open, section_is_open};
use super::{DebugState, DebugView, Gesture};
use cf_render::ActiveEvents;

/// Largeur du dock (px), celle de la maquette.
pub(crate) const DOCK_WIDTH: f32 = 250.0;

/// Clé retenue (`DebugPanelPrefs::open_families`) pour le dépliage de « Autres ».
pub(crate) const OTHERS_KEY: &str = "Autres";

/// Les sections repliables de « Réglages monde ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    /// Curseurs des façades.
    Facades,
    /// Choix de la voiture.
    Car,
}

impl Section {
    /// Toutes les sections, dans l'ordre d'affichage.
    pub(crate) const ALL: [Section; 2] = [Self::Facades, Self::Car];

    /// Identifiant stable, écrit dans les réglages : ne jamais le renommer (`DebugPanelPrefs::closed_sections`, déjà
    /// présent dans les fichiers de l'ancienne fenêtre).
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Facades => "facades",
            Self::Car => "car",
        }
    }

    /// Titre affiché.
    fn title(self) -> &'static str {
        match self {
            Self::Facades => "Façades",
            Self::Car => "Voiture",
        }
    }
}

/// Les ordres caméra qui existent, dans l'ordre des boutons.
pub(crate) fn camera_actions() -> [Action; 2] {
    [Action::SkipShot, Action::ShowDj]
}

/// Demande cette voiture et la retient dans les réglages. Ne fait rien (`false`) si elle est déjà celle qu'on veut, pas
/// encore décodée ou inconnue.
pub(crate) fn choose_car(selection: &mut CarSelection, settings: &mut AppSettings, key: &str) -> bool {
    if !selection.request(key) {
        return false;
    }
    settings.car = CarChoice::from(key.to_owned());
    true
}

/// Le dock, qui défile seul.
pub(crate) fn draw_dock(ui: &mut egui::Ui, view: &mut DebugView<'_>) {
    egui::ScrollArea::vertical()
        .id_salt("debug-dock")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                draw_cameras(ui, view.orders);
            });
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.strong("Voiture");
                draw_pad(ui, view.orders);
            });
            draw_others(ui, view);
            draw_world_settings(ui, view);
        });
}

fn draw_cameras(ui: &mut egui::Ui, orders: &mut Vec<Action>) {
    ui.strong("Plans caméra");
    let [skip, dj] = camera_actions();
    if ui
        .button("Plan suivant")
        .on_hover_text("Le réalisateur coupe vers un autre plan (comme la touche N).")
        .clicked()
    {
        push_order(orders, skip);
    }
    if ui
        .button("Enseigne DJ")
        .on_hover_text("Le réalisateur montre l'enseigne du DJ (comme la touche T).")
        .clicked()
    {
        push_order(orders, dj);
    }
    ui.weak("Choisir un plan / sa transition : pas encore pilotable");
}

/// Les événements d'une famille sans colonne (normalement aucun : le test du catalogue l'interdit).
fn draw_others(ui: &mut egui::Ui, view: &mut DebugView<'_>) {
    let Some(catalog) = view.catalog else {
        return;
    };
    let count = view.state.plan.others_len();
    if count == 0 {
        return;
    }
    let pinned = view.pinned();
    let remembered = view.settings.is_some_and(|s| family_is_open(s, OTHERS_KEY));
    let empty = ActiveEvents::default();
    let active = view.active.unwrap_or(&empty);
    let DebugState { plan, gestures, .. } = &mut *view.state;
    let response = egui::CollapsingHeader::new(format!("Autres ({count})"))
        .id_salt("debug-others")
        .default_open(remembered)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for event in plan.others(catalog) {
                    let run = chip_run(event.id, active, view.now_s);
                    let color = ui.visuals().selection.bg_fill;
                    match draw_chip(ui, event, pinned.contains(event.id), run, color) {
                        Some(ChipClick::Fire) => {
                            fire(view.orders, event);
                        }
                        Some(ChipClick::TogglePin) => gestures.push(Gesture::TogglePin(event.id)),
                        None => {}
                    }
                }
            });
        });
    let open = response.openness > 0.5;
    if view.settings.is_some() && open != remembered {
        gestures.push(Gesture::OthersOpen(open));
    }
}

/// « Réglages monde » : façades et voiture, chacune repliable et retenue.
fn draw_world_settings(ui: &mut egui::Ui, view: &mut DebugView<'_>) {
    ui.separator();
    ui.strong("Réglages monde");
    for section in Section::ALL {
        let remembered = view.settings.is_none_or(|s| section_is_open(s, section));
        let response = egui::CollapsingHeader::new(section.title())
            .id_salt(("debug-section", section.key()))
            .default_open(remembered)
            .show(ui, |ui| match section {
                Section::Facades => match view.tuning {
                    Some(tuning) => {
                        if let Some(next) = draw_tuning(ui, tuning) {
                            view.state.gestures.push(Gesture::Tuning(next));
                        }
                    }
                    None => {
                        ui.weak("Réglages des façades absents (monde absent).");
                    }
                },
                Section::Car => match view.cars {
                    Some(cars) => {
                        if let Some(key) = draw_cars(ui, cars) {
                            view.state.gestures.push(Gesture::Car(key));
                        }
                    }
                    None => {
                        ui.weak("Aucune voiture à choisir (monde absent).");
                    }
                },
            });
        let open = response.openness > 0.5;
        if view.settings.is_some() && open != remembered {
            view.state.gestures.push(Gesture::SectionOpen(section, open));
        }
    }
}

/// Les deux curseurs des façades. Rendent la nouvelle valeur seulement si elle bouge : le monde et les réglages
/// sauvegardés ne sont réveillés que par un geste de l'opérateur.
fn draw_tuning(ui: &mut egui::Ui, tuning: FacadeTuning) -> Option<FacadeTuning> {
    let mut next = tuning;
    ui.add(egui::Slider::new(&mut next.car_follow, 0.0..=1.0).text("Suivi de la voiture"));
    ui.add(egui::Slider::new(&mut next.window_life, 0.0..=1.0).text("Vie des fenêtres"));
    (next != tuning).then(|| next.clamped())
}

/// Les voitures proposées ; rend celle cliquée.
fn draw_cars(ui: &mut egui::Ui, selection: &CarSelection) -> Option<&'static str> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for option in &selection.options {
            let selected = option.key == selection.requested;
            let response = ui.add_enabled(option.ready, egui::Button::selectable(selected, option.label));
            if response.clicked() {
                picked = Some(option.key);
            }
            if !option.ready {
                response.on_disabled_hover_text("Cette voiture est encore en cours de décodage.");
            }
        }
    });
    picked
}
