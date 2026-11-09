//! L'application des gestes de l'onglet aux ressources : automatisme, registre, réglages, quartiers, façades, graffitis,
//! voiture. Chaque ressource n'est écrite que si le geste la change.

use bevy::prelude::*;
use cf_render::{
    ActiveEvents, CarSelection, DistrictOrder, FacadeTuning, GraffitiTuning, GraffitiWords, Settings, WorldAutomation,
};

use super::dock::{OTHERS_KEY, choose_car};
use super::persist::{family_is_open, section_is_open, set_family_open, set_section_open};
use super::{Gesture, graffiti};

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
            Gesture::District(_)
            | Gesture::Tuning(_)
            | Gesture::Graffiti(_)
            | Gesture::Car(_)
            | Gesture::Shot(_)
            | Gesture::Manual(_)
            | Gesture::Weather(_)
            | Gesture::Sign(_)
            | Gesture::Dj(_)
            | Gesture::Sandbox(_) => {
                return true;
            }
        }
        false
    });
}

/// Applique (et vide) les gestes restants : ordres de quartier, façades, voiture.
pub(crate) fn apply_world_gestures(
    gestures: &mut Vec<Gesture>,
    districts: &mut MessageWriter<DistrictOrder>,
    tuning: &mut Option<ResMut<FacadeTuning>>,
    graffiti: &mut Option<ResMut<GraffitiTuning>>,
    words: Option<&GraffitiWords>,
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
            Gesture::Graffiti(next) => {
                if let Some(graffiti) = graffiti.as_mut() {
                    // La ressource n'est écrite que si le geste la change.
                    if let Some(next) = graffiti::committed(graffiti, next, words) {
                        **graffiti = next;
                    }
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
