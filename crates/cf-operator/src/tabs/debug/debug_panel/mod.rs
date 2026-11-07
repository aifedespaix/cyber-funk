//! Fenêtre « Debug monde » : régler les façades, changer la voiture, piloter les quartiers et lancer à la main les
//! événements du monde (trafic, immeubles-vitrines…) pour voir ce qu'ils font.
//!
//! Elle n'existe qu'en Préparation (jamais en Live : un clic de trop ne doit pas lancer un événement devant le public) et
//! s'ouvre par `F4` ou par le bouton de l'en-tête. Un bouton « Lancer » écrit `Action::Trigger(id)`, le même chemin que le
//! MIDI : le limiteur de flashs reste aux commandes. Les ordres de quartier passent par `DistrictOrder`, lu par le monde.
//!
//! Tout le contenu défile dans une seule `ScrollArea` (petits écrans), en quatre sections repliables. Les sections
//! repliées, les familles ouvertes et la voiture choisie sont retenues dans les réglages sauvegardés ([`persist`]) ;
//! l'état des quartiers (gel, forçage, choix en cours) ne l'est jamais.

mod districts;
mod events;
mod persist;
#[cfg(test)]
mod tests;

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_render::{
    CarSelection, DistrictGradeState, DistrictOrder, DistrictState, FacadeTuning, Settings, WorldEventCatalog,
};
use cf_show::persist::{AppSettings, CarChoice};

use crate::OperatorMode;
use crate::live::LiveOrders;
use districts::{DistrictPick, district_orders, draw_districts};
use events::draw_events;
use persist::{section_is_open, set_section_open};

/// Les sections de la fenêtre, dans l'ordre d'affichage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    /// Curseurs des façades.
    Facades,
    /// Choix de la voiture.
    Car,
    /// Moteur de quartiers.
    Districts,
    /// Événements du catalogue.
    Events,
}

impl Section {
    /// Toutes les sections, dans l'ordre d'affichage.
    pub(crate) const ALL: [Section; 4] = [Self::Facades, Self::Car, Self::Districts, Self::Events];

    /// Identifiant stable, écrit dans les réglages : ne jamais le renommer.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Facades => "facades",
            Self::Car => "car",
            Self::Districts => "districts",
            Self::Events => "events",
        }
    }

    /// Titre affiché.
    fn title(self) -> &'static str {
        match self {
            Self::Facades => "Façades",
            Self::Car => "Voiture",
            Self::Districts => "Quartiers",
            Self::Events => "Événements",
        }
    }
}

/// Les sections montrées dans ce mode : aucune en Live.
pub(crate) fn sections(mode: OperatorMode) -> &'static [Section] {
    match mode {
        OperatorMode::Preparation => &Section::ALL,
        OperatorMode::Live => &[],
    }
}

/// État de la fenêtre (jamais sauvegardé).
#[derive(Resource, Debug, Default)]
pub(crate) struct DebugState {
    /// La fenêtre est ouverte.
    pub open: bool,
    filter: String,
    last_fired: Option<(&'static str, f64)>,
    /// Le quartier que l'opérateur s'apprête à forcer.
    district: DistrictPick,
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

/// Le bouton de l'en-tête qui ouvre ou ferme la fenêtre.
pub(crate) fn toggle_button(ui: &mut egui::Ui, state: &mut DebugState) {
    if ui.button("Debug monde  [F4]").clicked() {
        state.open = !state.open;
    }
}

/// La fenêtre « Debug monde ». Chaque section dit ce qui lui manque (monde absent) au lieu de s'afficher à vide.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
pub(crate) fn debug_window(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    catalog: Option<Res<WorldEventCatalog>>,
    selection: Option<ResMut<CarSelection>>,
    tuning: Option<ResMut<FacadeTuning>>,
    mut settings: Option<ResMut<Settings>>,
    (district, grades): (Option<Res<DistrictState>>, Option<Res<DistrictGradeState>>),
    mut district_writer: MessageWriter<DistrictOrder>,
    mut orders: ResMut<LiveOrders>,
    mut state: ResMut<DebugState>,
    mode: Res<OperatorMode>,
    time: Res<Time<Real>>,
) {
    let ctx = context.get_mut();
    if *mode != OperatorMode::Preparation {
        state.open = false;
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::F4)) {
        state.open = !state.open;
    }
    if !state.open {
        return;
    }
    let now = time.elapsed_secs_f64();
    let mut open = true;
    let state = &mut *state;
    let (mut tuning, mut selection) = (tuning, selection);
    egui::Window::new("Debug monde (F4)")
        .open(&mut open)
        .default_pos(egui::pos2(16.0, 48.0))
        .default_width(380.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                for &section in sections(*mode) {
                    let header = egui::CollapsingHeader::new(section.title())
                        .id_salt(("debug-section", section.key()))
                        .default_open(remembered_open(settings.as_deref(), section));
                    let response = header.show(ui, |ui| match section {
                        Section::Facades => match tuning.as_mut() {
                            // Écriture seulement sur un geste : sinon la ressource serait marquée changée à chaque image.
                            Some(tuning) => {
                                if let Some(next) = draw_tuning(ui, **tuning) {
                                    **tuning = next;
                                }
                            }
                            None => {
                                ui.weak("Réglages des façades absents (monde absent).");
                            }
                        },
                        Section::Car => match selection.as_mut() {
                            Some(selection) => draw_cars(ui, selection, &mut settings),
                            None => {
                                ui.weak("Aucune voiture à choisir (monde absent).");
                            }
                        },
                        Section::Districts => {
                            let library = grades.as_deref().map(DistrictGradeState::library);
                            let gestures = draw_districts(ui, district.as_deref(), library, &mut state.district);
                            district_writer.write_batch(district_orders(*mode, &state.district, &gestures));
                        }
                        Section::Events => match catalog.as_deref() {
                            Some(catalog) => draw_events(ui, catalog, state, &mut settings, &mut orders.0, now),
                            None => {
                                ui.weak("Aucun événement inscrit (monde absent).");
                            }
                        },
                    });
                    remember_open(&mut settings, section, response.openness > 0.5);
                }
            });
        });
    if !open {
        state.open = false;
    }
}

/// La section était-elle dépliée (vrai sans réglages) ?
fn remembered_open(settings: Option<&Settings>, section: Section) -> bool {
    settings.is_none_or(|s| section_is_open(&s.value, section))
}

/// Retient le dépliage d'une section. `ResMut` n'est déréférencé en écriture que sur un vrai changement : sinon
/// l'écriture automatique des réglages se réveillerait à chaque image.
fn remember_open(settings: &mut Option<ResMut<Settings>>, section: Section, open: bool) {
    if open != remembered_open(settings.as_deref(), section)
        && let Some(settings) = settings.as_mut()
    {
        set_section_open(&mut settings.value, section, open);
    }
}

/// Les deux curseurs des façades. Ils n'écrivent que si la valeur bouge : le monde et les réglages sauvegardés ne sont
/// réveillés que par un geste de l'opérateur.
fn draw_tuning(ui: &mut egui::Ui, tuning: FacadeTuning) -> Option<FacadeTuning> {
    let mut next = tuning;
    ui.add(egui::Slider::new(&mut next.car_follow, 0.0..=1.0).text("Suivi de la voiture"));
    ui.add(egui::Slider::new(&mut next.window_life, 0.0..=1.0).text("Vie des fenêtres"));
    (next != tuning).then(|| next.clamped())
}

fn draw_cars(ui: &mut egui::Ui, selection: &mut CarSelection, settings: &mut Option<ResMut<Settings>>) {
    let options = selection.options.clone();
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for option in &options {
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
    if let Some(key) = picked {
        match settings.as_mut() {
            Some(settings) => {
                // Écrire dans `value` réveille l'écriture automatique ; seulement si la demande a vraiment changé.
                let mut copy = settings.value.clone();
                if choose_car(selection, &mut copy, key) {
                    settings.value = copy;
                }
            }
            None => {
                selection.request(key);
            }
        }
    }
}
