//! Onglet Debug (Préparation seulement) : lancer à la main les événements du monde et voir ce qui tourne.
//!
//! ```text
//! barre du haut   mode manuel · En cours N (jauge) · recherche (/) · Tout arrêter
//! grille          Panneaux | Immeubles | Voitures | Quartiers      dock : plans caméra, pad voiture,
//!                 2/3 puces, 1/3 « En cours » par colonne                 Autres, Réglages monde
//! ```
//!
//! Le dessin ([`draw`]) ne lit que des références ([`DebugView`]) et n'écrit que deux choses : les ordres de l'image
//! (`Action`, plafonnés à `MAX_ORDERS_PER_FRAME`, tous par le chemin du MIDI : le limiteur reste aux commandes) et des
//! [`Gesture`]s, appliqués ensuite aux ressources ([`apply_gestures`], [`apply_world_gestures`]). Une ressource n'est
//! donc déréférencée en écriture que sur un vrai geste : les réglages sauvegardés (écriture automatique) et le monde
//! ne sont pas réveillés à chaque image. L'onglet est retiré du Live par la coque : un clic de trop ne doit pas lancer
//! un événement devant le public.

mod chip;
mod columns;
mod districts;
mod dock;
mod grid;
mod lane;
mod pad;
mod persist;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_world;
mod top_bar;

use std::collections::BTreeSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use cf_control::Action;
use cf_render::{
    ActiveEvents, ActiveId, CarSelection, DistrictGradeState, DistrictOrder, DistrictState, EventCategory,
    FacadeTuning, GradeLibrary, Settings, SimClock, WorldAutomation, WorldEventCatalog,
};
use cf_show::persist::AppSettings;

use columns::PlanCache;
use districts::DistrictPick;
pub(crate) use dock::Section;
#[cfg(test)]
pub(crate) use dock::camera_actions;
use dock::{DOCK_WIDTH, OTHERS_KEY, choose_car};
use grid::{GAP, in_rect, split_dock};
use persist::{family_is_open, section_is_open, set_family_open, set_section_open};

/// Les épingles quand les réglages manquent.
static NO_PINS: BTreeSet<String> = BTreeSet::new();

/// État de l'onglet (jamais sauvegardé).
#[derive(Resource, Debug, Default)]
pub(crate) struct DebugState {
    /// Texte de la recherche.
    filter: String,
    /// Le quartier que l'opérateur s'apprête à demander.
    district: DistrictPick,
    /// Rangement du catalogue, recalculé seulement quand le filtre, les épingles ou le catalogue changent.
    plan: PlanCache,
    /// Gestes de l'image, vidés par leur application (tampon réutilisé).
    gestures: Vec<Gesture>,
}

/// Un geste de l'opérateur, appliqué après le dessin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Gesture {
    /// Mode manuel (`WorldAutomation::manual`).
    SetManual(bool),
    /// Vider le registre des événements en cours.
    StopAll,
    /// Retirer du registre une colonne.
    StopColumn(EventCategory),
    /// Retirer du registre une entrée.
    Stop(ActiveId),
    /// Épingler ou désépingler (`OperatorPrefs::pinned_events`).
    TogglePin(&'static str),
    /// Ordre au moteur de quartiers.
    District(DistrictOrder),
    /// Nouveaux curseurs des façades.
    Tuning(FacadeTuning),
    /// Voiture demandée.
    Car(&'static str),
    /// Dépliage d'une section de « Réglages monde ».
    SectionOpen(Section, bool),
    /// Dépliage de « Autres ».
    OthersOpen(bool),
}

/// Ce que le dessin lit. Tout est facultatif sauf l'état et la file d'ordres : un plugin absent (monde, réglages) se dit
/// en clair dans la colonne concernée.
#[derive(Debug)]
pub(crate) struct DebugView<'a> {
    pub state: &'a mut DebugState,
    pub catalog: Option<&'a WorldEventCatalog>,
    pub active: Option<&'a ActiveEvents>,
    pub manual: Option<bool>,
    /// Temps de simulation (s), celui de `ActiveEvents`.
    pub now_s: f64,
    pub settings: Option<&'a AppSettings>,
    pub district: Option<&'a DistrictState>,
    pub library: Option<&'a GradeLibrary>,
    pub cars: Option<&'a CarSelection>,
    pub tuning: Option<FacadeTuning>,
    /// Ordres de l'image (`LiveOrders`).
    pub orders: &'a mut Vec<Action>,
}

impl<'a> DebugView<'a> {
    /// Les événements épinglés.
    fn pinned(&self) -> &'a BTreeSet<String> {
        self.settings.map_or(&NO_PINS, |s| &s.operator.pinned_events)
    }
}

/// L'onglet : barre du haut, puis la grille et le dock sur toute la hauteur restante.
pub(crate) fn draw(ui: &mut egui::Ui, view: &mut DebugView<'_>) {
    let bar = top_bar::TopBar {
        manual: view.manual,
        running: view.active.map(ActiveEvents::len),
    };
    let pinned = view.pinned();
    let DebugState {
        filter, plan, gestures, ..
    } = &mut *view.state;
    top_bar::draw(ui, bar, filter, gestures);
    if let Some(catalog) = view.catalog {
        plan.refresh(catalog, filter, pinned);
    }
    ui.separator();
    let (grid_rect, dock_rect) = split_dock(ui.available_rect_before_wrap(), DOCK_WIDTH, GAP);
    grid::draw_grid(ui, grid_rect, view);
    in_rect(ui, dock_rect, |ui| dock::draw_dock(ui, view));
}

/// Applique les gestes qui touchent l'automatisme, le registre et les réglages de l'interface ; les autres (quartiers,
/// façades, voiture) restent pour [`apply_world_gestures`]. Chaque ressource n'est écrite que si le geste la change.
pub(crate) fn apply_gestures(
    gestures: &mut Vec<Gesture>,
    automation: &mut Option<ResMut<WorldAutomation>>,
    active: &mut Option<ResMut<ActiveEvents>>,
    settings: &mut Option<ResMut<Settings>>,
) {
    gestures.retain(|gesture| {
        match *gesture {
            Gesture::SetManual(manual) => {
                if let Some(automation) = automation.as_mut() {
                    automation.set_if_neq(WorldAutomation { manual });
                }
            }
            Gesture::StopAll => {
                if let Some(active) = active.as_mut() {
                    active.clear();
                }
            }
            Gesture::StopColumn(category) => {
                if let Some(active) = active.as_mut() {
                    active.end_category(category);
                }
            }
            Gesture::Stop(id) => {
                if let Some(active) = active.as_mut() {
                    active.end(id);
                }
            }
            Gesture::TogglePin(id) => {
                if let Some(settings) = settings.as_mut() {
                    let pinned = &mut settings.value.operator.pinned_events;
                    if !pinned.remove(id) {
                        pinned.insert(id.to_owned());
                    }
                }
            }
            Gesture::SectionOpen(section, open) => {
                if let Some(settings) = settings.as_mut()
                    && section_is_open(&settings.value, section) != open
                {
                    set_section_open(&mut settings.value, section, open);
                }
            }
            Gesture::OthersOpen(open) => {
                if let Some(settings) = settings.as_mut()
                    && family_is_open(&settings.value, OTHERS_KEY) != open
                {
                    set_family_open(&mut settings.value, OTHERS_KEY, open);
                }
            }
            Gesture::District(_) | Gesture::Tuning(_) | Gesture::Car(_) => return true,
        }
        false
    });
}

/// Applique (et vide) les gestes restants : ordres de quartier, façades, voiture.
pub(crate) fn apply_world_gestures(
    gestures: &mut Vec<Gesture>,
    districts: &mut MessageWriter<DistrictOrder>,
    tuning: &mut Option<ResMut<FacadeTuning>>,
    cars: &mut Option<ResMut<CarSelection>>,
    settings: &mut Option<ResMut<Settings>>,
) {
    for gesture in gestures.drain(..) {
        match gesture {
            Gesture::District(order) => {
                districts.write(order);
            }
            Gesture::Tuning(next) => {
                if let Some(tuning) = tuning.as_mut() {
                    tuning.set_if_neq(next);
                }
            }
            Gesture::Car(key) => {
                let Some(selection) = cars.as_mut() else {
                    continue;
                };
                match settings.as_mut() {
                    Some(settings) => {
                        // Écrire dans `value` réveille l'écriture automatique : seulement si la demande a changé.
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
            _ => {}
        }
    }
}

/// Les ressources propres à l'onglet (l'automatisme, le registre et l'horloge viennent de la coque, qui les lit déjà).
#[derive(SystemParam)]
pub(crate) struct DebugParams<'w> {
    state: ResMut<'w, DebugState>,
    catalog: Option<Res<'w, WorldEventCatalog>>,
    settings: Option<ResMut<'w, Settings>>,
    district: Option<Res<'w, DistrictState>>,
    grades: Option<Res<'w, DistrictGradeState>>,
    districts: MessageWriter<'w, DistrictOrder>,
    tuning: Option<ResMut<'w, FacadeTuning>>,
    cars: Option<ResMut<'w, CarSelection>>,
}

/// Les ressources partagées avec la coque.
#[allow(missing_debug_implementations)] // Des accès aux ressources, rien à afficher.
pub(crate) struct Shared<'a, 'w> {
    pub automation: &'a mut Option<ResMut<'w, WorldAutomation>>,
    pub active: &'a mut Option<ResMut<'w, ActiveEvents>>,
    pub clock: Option<&'a SimClock>,
}

/// Dessine l'onglet dans le panneau central puis applique les gestes de l'image.
pub(crate) fn show(root: &mut egui::Ui, params: &mut DebugParams, shared: Shared<'_, '_>, orders: &mut Vec<Action>) {
    let DebugParams {
        state,
        catalog,
        settings,
        district,
        grades,
        districts,
        tuning,
        cars,
    } = params;
    egui::CentralPanel::default().show(root, |ui| {
        let mut view = DebugView {
            state,
            catalog: catalog.as_deref(),
            active: shared.active.as_deref(),
            manual: shared.automation.as_deref().map(|a| a.manual),
            now_s: shared.clock.map_or(0.0, SimClock::sim_s),
            settings: settings.as_deref().map(|s| &s.value),
            district: district.as_deref(),
            library: grades.as_deref().map(DistrictGradeState::library),
            cars: cars.as_deref(),
            tuning: tuning.as_deref().copied(),
            orders,
        };
        draw(ui, &mut view);
    });
    if state.gestures.is_empty() {
        return;
    }
    let gestures = &mut state.gestures;
    apply_gestures(gestures, shared.automation, shared.active, settings);
    apply_world_gestures(gestures, districts, tuning, cars, settings);
}
