//! Fenêtre « Debug monde » : lancer à la main les événements du monde (trafic, immeubles-vitrines…) pour voir ce qu'ils
//! font, et changer la voiture.
//!
//! Elle n'existe qu'en Préparation (jamais en Live : un clic de trop ne doit pas lancer un événement devant le public) et
//! s'ouvre par `F4` ou par le bouton de l'en-tête. Un bouton « Lancer » écrit `Action::Trigger(id)`, le même chemin que le
//! MIDI : le limiteur de flashs reste aux commandes. Les familles ouvertes et la voiture choisie sont retenues dans les
//! réglages sauvegardés.

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_control::{Action, TriggerId};
use cf_render::{CarSelection, FacadeTuning, Settings, WorldEvent, WorldEventCatalog};
use cf_show::persist::{AppSettings, CarChoice};

use crate::OperatorMode;
use crate::live::LiveOrders;

/// Au plus autant d'ordres par image : une rafale de clics ne remplit jamais la file du chef du spectacle.
pub(crate) const MAX_ORDERS_PER_FRAME: usize = 8;

/// Combien de temps (s) le dernier événement lancé reste affiché en haut de la fenêtre.
const LAST_FIRED_SHOWN_S: f64 = 3.0;

/// Préfixe, dans les réglages, des familles ouvertes.
const OPEN_PREFIX: &str = "debug-open:";

/// État de la fenêtre.
#[derive(Resource, Debug, Default)]
pub(crate) struct DebugState {
    /// La fenêtre est ouverte.
    pub open: bool,
    filter: String,
    last_fired: Option<(&'static str, f64)>,
}

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

/// Écrit l'ordre de lancer cet événement. `false` quand la file de l'image est pleine : le clic est perdu sans bruit.
pub(crate) fn fire(orders: &mut Vec<Action>, event: &WorldEvent) -> bool {
    if orders.len() >= MAX_ORDERS_PER_FRAME {
        return false;
    }
    orders.push(Action::Trigger(TriggerId::new(event.id)));
    true
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

fn family_is_open(settings: &AppSettings, family: &str) -> bool {
    settings
        .collapsed_panels
        .iter()
        .any(|entry| entry.strip_prefix(OPEN_PREFIX) == Some(family))
}

fn set_family_open(settings: &mut AppSettings, family: &str, open: bool) {
    let entry = format!("{OPEN_PREFIX}{family}");
    let known = settings.collapsed_panels.contains(&entry);
    match (open, known) {
        (true, false) => settings.collapsed_panels.push(entry),
        (false, true) => settings.collapsed_panels.retain(|e| *e != entry),
        _ => {}
    }
}

/// Le bouton de l'en-tête qui ouvre ou ferme la fenêtre.
pub(crate) fn toggle_button(ui: &mut egui::Ui, state: &mut DebugState) {
    if ui.button("Debug monde  [F4]").clicked() {
        state.open = !state.open;
    }
}

/// La fenêtre « Debug monde ». Sans catalogue (monde absent), elle le dit au lieu de s'afficher à vide.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
pub(crate) fn debug_window(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    catalog: Option<Res<WorldEventCatalog>>,
    selection: Option<ResMut<CarSelection>>,
    tuning: Option<ResMut<FacadeTuning>>,
    settings: Option<ResMut<Settings>>,
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
    let mut settings = settings;
    egui::Window::new("Debug monde (F4)")
        .open(&mut open)
        .default_pos(egui::pos2(16.0, 48.0))
        .default_width(380.0)
        .show(ctx, |ui| {
            if let Some(mut tuning) = tuning {
                if let Some(next) = draw_tuning(ui, *tuning) {
                    // Écriture seulement sur un geste : sinon la ressource serait marquée changée à chaque image.
                    *tuning = next;
                }
                ui.separator();
            }
            if let Some(mut selection) = selection {
                draw_cars(ui, &mut selection, settings.as_deref_mut());
                ui.separator();
            }
            let Some(catalog) = catalog else {
                ui.weak("Aucun événement inscrit (monde absent).");
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Recherche");
                ui.text_edit_singleline(&mut state.filter);
            });
            if let Some((label, at)) = state.last_fired
                && now - at < LAST_FIRED_SHOWN_S
            {
                ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Lancé : {label}"));
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                draw_events(ui, &catalog, state, settings.as_deref_mut(), &mut orders.0, now);
            });
        });
    if !open {
        state.open = false;
    }
}

/// Les deux curseurs des façades. Ils n'écrivent que si la valeur bouge : le monde et les réglages sauvegardés ne sont
/// réveillés que par un geste de l'opérateur.
fn draw_tuning(ui: &mut egui::Ui, tuning: FacadeTuning) -> Option<FacadeTuning> {
    ui.strong("Façades");
    let mut next = tuning;
    ui.add(egui::Slider::new(&mut next.car_follow, 0.0..=1.0).text("Suivi de la voiture"));
    ui.add(egui::Slider::new(&mut next.window_life, 0.0..=1.0).text("Vie des fenêtres"));
    (next != tuning).then(|| next.clamped())
}

fn draw_cars(ui: &mut egui::Ui, selection: &mut CarSelection, settings: Option<&mut Settings>) {
    ui.strong("Voiture");
    let options = selection.options.clone();
    let mut picked = None;
    ui.horizontal(|ui| {
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
        match settings {
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

fn draw_events(
    ui: &mut egui::Ui,
    catalog: &WorldEventCatalog,
    state: &mut DebugState,
    mut settings: Option<&mut Settings>,
    orders: &mut Vec<Action>,
    now: f64,
) {
    let searching = !state.filter.trim().is_empty();
    for (family, events) in plan(catalog, &state.filter) {
        let remembered = settings.as_deref().is_some_and(|s| family_is_open(&s.value, family));
        let mut header = egui::CollapsingHeader::new(format!("{family} ({})", events.len()))
            .id_salt(family)
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
        if !searching && let Some(settings) = settings.as_deref_mut() {
            let open = response.openness > 0.5;
            if open != family_is_open(&settings.value, family) {
                set_family_open(&mut settings.value, family, open);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cf_render::CarOption;

    use super::*;

    fn event(id: &'static str, label: &'static str, family: &'static str) -> WorldEvent {
        WorldEvent {
            id,
            label,
            family,
            note: "",
        }
    }

    fn catalog() -> WorldEventCatalog {
        let mut catalog = WorldEventCatalog::default();
        catalog.register(event("storm", "Orage", "Trafic"));
        catalog.register(event("fish", "Banc de poissons", "Trafic"));
        catalog.register(event("landmark:arcade", "Borne d'arcade", "Immeubles"));
        catalog
    }

    #[test]
    fn le_filtre_garde_les_familles_qui_ont_une_correspondance() {
        let catalog = catalog();
        let all = plan(&catalog, "");
        assert_eq!(
            all.iter().map(|(f, e)| (*f, e.len())).collect::<Vec<_>>(),
            [("Trafic", 2), ("Immeubles", 1)]
        );
        let arcade = plan(&catalog, "arcade");
        assert_eq!(arcade.len(), 1);
        assert_eq!(arcade[0].0, "Immeubles");
        assert_eq!(plan(&catalog, "introuvable").len(), 0);
    }

    #[test]
    fn un_clic_sur_lancer_ecrit_une_action_trigger_avec_l_id_de_l_evenement() {
        let mut orders = Vec::new();
        assert!(fire(&mut orders, &event("storm", "Orage", "Trafic")));
        assert_eq!(orders, [Action::Trigger(TriggerId::new("storm"))]);
    }

    #[test]
    fn la_file_d_ordres_de_l_image_est_bornee() {
        let mut orders = Vec::new();
        let storm = event("storm", "Orage", "Trafic");
        let accepted = (0..100).filter(|_| fire(&mut orders, &storm)).count();
        assert_eq!(accepted, MAX_ORDERS_PER_FRAME);
        assert_eq!(orders.len(), MAX_ORDERS_PER_FRAME);
    }

    fn selection() -> CarSelection {
        CarSelection {
            options: vec![
                CarOption {
                    key: "dirty",
                    label: "Dirty car",
                    ready: true,
                },
                CarOption {
                    key: "cyber",
                    label: "CyberCar",
                    ready: true,
                },
            ],
            active: "dirty",
            requested: "dirty",
        }
    }

    #[test]
    fn choisir_la_voiture_en_place_n_ecrit_rien() {
        let mut selection = selection();
        let mut settings = AppSettings::default();
        assert!(!choose_car(&mut selection, &mut settings, "dirty"));
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn choisir_une_autre_voiture_la_demande_et_la_retient() {
        let mut selection = selection();
        let mut settings = AppSettings::default();
        assert!(choose_car(&mut selection, &mut settings, "cyber"));
        assert_eq!(selection.requested, "cyber");
        assert_eq!(settings.car, CarChoice::Cyber);
    }

    #[test]
    fn les_familles_ouvertes_se_retiennent_sans_doublon() {
        let mut settings = AppSettings::default();
        set_family_open(&mut settings, "Trafic", true);
        set_family_open(&mut settings, "Trafic", true);
        assert!(family_is_open(&settings, "Trafic"));
        assert_eq!(settings.collapsed_panels.len(), 1);
        set_family_open(&mut settings, "Trafic", false);
        assert!(!family_is_open(&settings, "Trafic"));
    }
}
