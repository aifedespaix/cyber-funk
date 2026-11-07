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

/// Largeur (px) à partir de laquelle la barre montre la phrase Auto/Manuel, la mention « charge » et des champs larges.
/// En dessous (fenêtre de 640 px au plus étroit), elle se resserre pour que rien ne se chevauche.
const WIDE_BAR: f32 = 1000.0;

/// Les dimensions de la barre pour une largeur disponible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BarLayout {
    /// La phrase qui explique le mode (sinon : infobulle de la bascule seulement).
    pub sentence: bool,
    /// La mention « charge » / « charge élevée » à côté de la jauge.
    pub load_label: bool,
    /// Largeur du champ de recherche (px).
    pub search_width: f32,
    /// Largeur de la jauge (px).
    pub gauge_width: f32,
}

impl BarLayout {
    /// Large ou resserrée selon `width`.
    pub(crate) fn for_width(width: f32) -> Self {
        if width >= WIDE_BAR {
            Self {
                sentence: true,
                load_label: true,
                search_width: SEARCH_WIDTH,
                gauge_width: 90.0,
            }
        } else {
            Self {
                sentence: false,
                load_label: false,
                search_width: 120.0,
                gauge_width: 50.0,
            }
        }
    }
}

/// Où sont tombés les deux boutons extrêmes (lu par les tests de mise en page).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BarRects {
    /// La bascule « Mode manuel », à gauche.
    pub toggle: egui::Rect,
    /// « Tout arrêter », à droite.
    pub stop: egui::Rect,
}

/// La barre. Le filtre est édité en place ; les gestes s'ajoutent à `gestures`. Les éléments de droite sont posés
/// d'abord, la bascule et sa phrase (tronquée) prennent ce qui reste : rien ne se chevauche sur une fenêtre étroite.
pub(crate) fn draw(ui: &mut egui::Ui, bar: TopBar, filter: &mut String, gestures: &mut Vec<Gesture>) -> BarRects {
    let typing = ui.ctx().egui_wants_keyboard_input();
    let focus = ui.input(|input| wants_search_focus(input, typing));
    let layout = BarLayout::for_width(ui.available_width());
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let stop = ui.button("■ Tout arrêter").on_hover_text(concat!(
                "Vide la liste des événements en cours. Le monde ne sait pas arrêter un événement en route : ",
                "ses directeurs finissent eux-mêmes ce qui est lancé."
            ));
            if stop.clicked() {
                gestures.push(Gesture::StopAll);
            }
            let search = ui.add(
                egui::TextEdit::singleline(filter)
                    .id_salt("debug-search")
                    .hint_text("Rechercher  ( / )")
                    .desired_width(layout.search_width),
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
                    if layout.load_label {
                        ui.weak(if ratio > LOAD_HIGH { "charge élevée" } else { "charge" });
                    }
                    ui.add(egui::ProgressBar::new(ratio).desired_width(layout.gauge_width));
                    ui.strong(running.to_string());
                }
                None => {
                    ui.weak("—");
                }
            }
            ui.label("En cours");
            let toggle = ui
                .with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    draw_mode(ui, bar.manual, layout.sentence, gestures)
                })
                .inner;
            BarRects {
                toggle,
                stop: stop.rect,
            }
        })
        .inner
    })
    .inner
}

/// La bascule « Mode manuel » et, s'il y a la place, la phrase qui dit ce qu'elle fait. Rend le rectangle de la bascule.
fn draw_mode(ui: &mut egui::Ui, manual: Option<bool>, sentence: bool, gestures: &mut Vec<Gesture>) -> egui::Rect {
    let mut on = manual.unwrap_or(false);
    let explain = if on {
        "Manuel : seuls tes clics lancent."
    } else {
        "Auto : le monde lance selon la musique ; tes clics s'ajoutent."
    };
    let toggle = ui
        .add_enabled(manual.is_some(), egui::Button::selectable(on, "Mode manuel"))
        .on_hover_text(explain)
        .on_hover_text("Même bascule que la barre de statut : le monde ne lance plus rien de lui-même.")
        .on_disabled_hover_text("Automatisme du monde absent (monde absent).");
    if toggle.clicked() {
        on = !on;
        gestures.push(Gesture::SetManual(on));
    }
    if sentence {
        ui.add(egui::Label::new(egui::RichText::new(explain).weak()).truncate());
    }
    toggle.rect
}
