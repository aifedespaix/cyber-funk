//! La barre du haut de l'onglet Debug : mode manuel, jauge des événements en cours, recherche (`/`), « Tout arrêter ».
//!
//! La bascule écrit la même ressource que celle de la barre de statut (`WorldAutomation`). « Tout arrêter » vide le
//! registre `ActiveEvents` : aucun directeur du monde ne sait arrêter un événement en route, chacun finit les siens.

use bevy_egui::egui;

use super::Gesture;

/// Nombre d'événements en cours pour lequel la jauge est pleine (« charge élevée » au-delà de 70 %).
const LOAD_FULL: usize = 8;

/// Seuil de la mention « charge élevée ».
const LOAD_HIGH: f32 = 0.7;

/// Largeur du champ de recherche (px).
const SEARCH_WIDTH: f32 = 190.0;

/// Remplissage de la jauge, 0..=1.
pub(crate) fn load_ratio(running: usize) -> f32 {
    (running as f32 / LOAD_FULL as f32).min(1.0)
}

/// `/` tapé hors d'une saisie : la recherche prend le focus. Lu comme texte, pas comme touche : `/` est sur Maj+`:`
/// en AZERTY, le caractère est le seul repère commun. Le clavier par défaut ne lie pas `/`.
pub(crate) fn wants_search_focus(input: &egui::InputState, typing: bool) -> bool {
    !typing
        && input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Text(text) if text == "/"))
}

/// Ce que la barre affiche.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TopBar {
    /// Mode manuel (`None` : ressource absente, bascule grisée).
    pub manual: Option<bool>,
    /// Événements en cours (`None` : registre absent).
    pub running: Option<usize>,
}

/// La barre. Le filtre est édité en place ; les gestes s'ajoutent à `gestures`.
pub(crate) fn draw(ui: &mut egui::Ui, bar: TopBar, filter: &mut String, gestures: &mut Vec<Gesture>) {
    let typing = ui.ctx().egui_wants_keyboard_input();
    let focus = ui.input(|input| wants_search_focus(input, typing));
    ui.horizontal(|ui| {
        let mut manual = bar.manual.unwrap_or(false);
        let toggle = ui
            .add_enabled(bar.manual.is_some(), egui::Button::selectable(manual, "Mode manuel"))
            .on_hover_text("Même bascule que la barre de statut : le monde ne lance plus rien de lui-même.")
            .on_disabled_hover_text("Automatisme du monde absent (monde absent).");
        if toggle.clicked() {
            manual = !manual;
            gestures.push(Gesture::SetManual(manual));
        }
        ui.weak(if manual {
            "Manuel : seuls tes clics lancent."
        } else {
            "Auto : le monde lance selon la musique ; tes clics s'ajoutent."
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("■ Tout arrêter")
                .on_hover_text(concat!(
                    "Vide la liste des événements en cours. Le monde ne sait pas arrêter un événement en route : ",
                    "ses directeurs finissent eux-mêmes ce qui est lancé."
                ))
                .clicked()
            {
                gestures.push(Gesture::StopAll);
            }
            let search = ui.add(
                egui::TextEdit::singleline(filter)
                    .id_salt("debug-search")
                    .hint_text("Rechercher  ( / )")
                    .desired_width(SEARCH_WIDTH),
            );
            // Après le dessin : le `/` de cette image n'atterrit pas dans le champ.
            if focus {
                search.request_focus();
            }
            if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                filter.clear();
            }
            match bar.running {
                Some(running) => {
                    let ratio = load_ratio(running);
                    let label = if ratio > LOAD_HIGH { "charge élevée" } else { "charge" };
                    ui.weak(label);
                    ui.add(egui::ProgressBar::new(ratio).desired_width(90.0));
                    ui.strong(running.to_string());
                }
                None => {
                    ui.weak("—");
                }
            }
            ui.label("En cours");
        });
    });
}
