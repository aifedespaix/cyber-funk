//! Section « Événements » : lancer à la main les événements du monde, rangés par famille, avec une recherche.

use bevy::prelude::*;
use bevy_egui::egui;
use cf_control::{Action, TriggerId};
use cf_render::{Settings, WorldEvent, WorldEventCatalog};

use super::DebugState;
use super::persist::{family_is_open, set_family_open};

/// Au plus autant d'ordres par image : une rafale de clics ne remplit jamais la file du chef du spectacle.
pub(crate) const MAX_ORDERS_PER_FRAME: usize = 8;

/// Combien de temps (s) le dernier événement lancé reste affiché en haut de la section.
const LAST_FIRED_SHOWN_S: f64 = 3.0;

/// Familles du catalogue cachées de la liste générique : « Quartiers » (`district:<type>`, inscrite par le monde
/// pour le MIDI) a sa propre section, plus complète (variante, transition) ; la montrer deux fois prêterait à
/// confusion. Les déclencheurs restent au catalogue, donc au MIDI et à la recherche du learn.
pub(crate) const HIDDEN_FAMILIES: [&str; 1] = ["Quartiers"];

/// Les événements à montrer, rangés par famille : seules les familles qui ont une correspondance restent.
pub(crate) fn plan<'a>(catalog: &'a WorldEventCatalog, filter: &str) -> Vec<(&'static str, Vec<&'a WorldEvent>)> {
    let matching = catalog.search(filter);
    catalog
        .families()
        .into_iter()
        .filter_map(|family| {
            let events: Vec<&WorldEvent> = matching
                .iter()
                .copied()
                .filter(|event| event.family == family)
                .collect();
            (!events.is_empty()).then_some((family, events))
        })
        .collect()
}

/// Comme [`plan`], sans les familles qui ont leur propre section ([`HIDDEN_FAMILIES`]).
pub(crate) fn visible_plan<'a>(
    catalog: &'a WorldEventCatalog,
    filter: &str,
) -> Vec<(&'static str, Vec<&'a WorldEvent>)> {
    let mut families = plan(catalog, filter);
    families.retain(|(family, _)| !HIDDEN_FAMILIES.contains(family));
    families
}

/// Écrit l'ordre de lancer cet événement. `false` quand la file de l'image est pleine : le clic est perdu sans bruit.
pub(crate) fn fire(orders: &mut Vec<Action>, event: &WorldEvent) -> bool {
    if orders.len() >= MAX_ORDERS_PER_FRAME {
        return false;
    }
    orders.push(Action::Trigger(TriggerId::new(event.id)));
    true
}

/// Le contenu de la section : recherche, dernier lancé, familles repliables.
pub(super) fn draw_events(
    ui: &mut egui::Ui,
    catalog: &WorldEventCatalog,
    state: &mut DebugState,
    settings: &mut Option<ResMut<Settings>>,
    orders: &mut Vec<Action>,
    now: f64,
) {
    ui.horizontal(|ui| {
        ui.label("Recherche");
        ui.text_edit_singleline(&mut state.filter);
    });
    if let Some((label, at)) = state.last_fired
        && now - at < LAST_FIRED_SHOWN_S
    {
        ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Lancé : {label}"));
    }
    let searching = !state.filter.trim().is_empty();
    for (family, events) in visible_plan(catalog, &state.filter) {
        let remembered = settings.as_deref().is_some_and(|s| family_is_open(&s.value, family));
        let mut header = egui::CollapsingHeader::new(format!("{family} ({})", events.len()))
            .id_salt(("debug-family", family))
            .default_open(remembered);
        if searching {
            // Une recherche montre ses résultats : tout s'ouvre, sans toucher à ce qu'on retient.
            header = header.open(Some(true));
        }
        let response = header.show(ui, |ui| {
            for event in &events {
                ui.horizontal(|ui| {
                    if ui.button("Lancer").clicked() && fire(orders, event) {
                        state.last_fired = Some((event.label, now));
                    }
                    ui.label(event.label);
                    if !event.note.is_empty() {
                        ui.weak(event.note);
                    }
                });
            }
        });
        let open = response.openness > 0.5;
        // `ResMut` n'est déréférencé en écriture que sur un vrai changement : sinon l'écriture automatique des
        // réglages se réveillerait à chaque image.
        if !searching
            && remembered != open
            && let Some(settings) = settings.as_mut()
        {
            set_family_open(&mut settings.value, family, open);
        }
    }
}
