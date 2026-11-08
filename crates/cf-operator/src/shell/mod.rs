//! La coque de la fenêtre opérateur : barre d'onglets (Live, DJs, Debug), badge de mise à jour, raccourcis.
//!
//! ```text
//! EguiPrimaryContextPass :
//!   shell_input   Ctrl+1/2/3 (onglets), M (repère)  ─▶ ShellState, journal
//!   main_panel    en-tête (onglets + badge) puis l'onglet affiché
//!   persist_tab   ShellState::tab ─▶ Settings.value.operator.last_tab (seulement s'il a changé)
//! ```
//!
//! Trois onglets, tous toujours présents : Live pour piloter, DJs pour éditer la soirée, Debug pour le monde et les
//! statistiques.

pub(crate) mod confirm;
pub(crate) mod shortcuts;
#[cfg(test)]
mod tests;
pub(crate) mod update_badge;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_render::{
    ActiveEvents, DebugLog, DiagnosticsSnapshot, MeasurementSession, Settings, SimClock, SimControl, WorldAutomation,
};

use crate::quit::QuitPrompt;
use crate::services::logbuf::{self, OperatorLog};
use crate::tabs::stats::LogViewState;
use crate::tabs::stats::diagnostics::PanelState;
use shortcuts::ShellKey;
pub use update_badge::{InstallUpdate, UpdateStatus};

/// Les onglets de la fenêtre opérateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tab {
    /// Piloter le show : DJ, son, démarrer et arrêter.
    #[default]
    Live,
    /// Créer et modifier les DJ de la soirée.
    Djs,
    /// Le monde et les statistiques, pour comprendre ce qui se passe.
    Debug,
}

impl Tab {
    /// Les onglets, dans l'ordre de la barre.
    pub(crate) const ALL: [Tab; 3] = [Self::Live, Self::Djs, Self::Debug];

    /// Identifiant stable, écrit dans `OperatorPrefs::last_tab` : ne jamais le renommer.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Djs => "djs",
            Self::Debug => "debug",
        }
    }

    /// L'onglet de cet identifiant, `None` s'il est inconnu.
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.key() == key)
    }

    /// L'onglet à rouvrir au démarrage : celui retenu, ou Live si la valeur est inconnue. Les identifiants des
    /// anciennes versions (`settings`, `stats`) retombent donc sur Live.
    pub(crate) fn restore(key: &str) -> Self {
        Self::from_key(key).unwrap_or_default()
    }

    /// Libellé de la barre d'onglets.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Live => "Live",
            Self::Djs => "DJs",
            Self::Debug => "Debug",
        }
    }
}

/// Ce que l'onglet Debug affiche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DebugView {
    /// Le monde : quartiers, événements, réglages.
    #[default]
    World,
    /// Les statistiques : tuiles, cartes, journal de session.
    Stats,
}

/// L'état de la coque (jamais sauvegardé, sauf l'onglet par `persist_tab`).
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub(crate) struct ShellState {
    /// Onglet choisi par l'opérateur.
    pub tab: Tab,
    /// Vue de l'onglet Debug.
    pub debug_view: DebugView,
}

impl ShellState {
    /// Choisit un onglet. `false` si rien ne change.
    pub(crate) fn select(&mut self, tab: Tab) -> bool {
        if self.tab == tab {
            return false;
        }
        self.tab = tab;
        true
    }
}

/// Branche la coque. Ajouté par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.init_resource::<ShellState>().add_systems(Startup, restore_tab);
}

/// Rouvre l'onglet de la dernière session.
pub(crate) fn restore_tab(settings: Option<Res<Settings>>, mut state: ResMut<ShellState>) {
    if let Some(settings) = settings {
        state.tab = Tab::restore(&settings.value.operator.last_tab);
    }
}

/// Retient l'onglet choisi. Les réglages ne sont touchés (donc réécrits sur disque) que sur un vrai changement.
pub(crate) fn persist_tab(state: Res<ShellState>, settings: Option<ResMut<Settings>>) {
    let Some(mut settings) = settings else {
        return;
    };
    let key = state.tab.key();
    if state.is_changed() && settings.value.operator.last_tab != key {
        key.clone_into(&mut settings.value.operator.last_tab);
    }
}

/// Les touches globales. Rien pendant une saisie de texte.
pub(crate) fn shell_input(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    quit: Option<Res<QuitPrompt>>,
    mut state: ResMut<ShellState>,
    mut pressed: Local<Vec<ShellKey>>,
    marks: MarkParams,
    mut log_view: ResMut<LogViewState>,
) {
    let ctx = context.get_mut();
    // Pendant le dialogue de sortie, le clavier lui appartient.
    let dialog_open = quit.is_some_and(|q| q.pending.is_some());
    let typing = ctx.egui_wants_keyboard_input() || dialog_open;
    pressed.clear();
    ctx.input(|input| shortcuts::poll(input, typing, &mut pressed));
    for key in pressed.drain(..) {
        match key {
            ShellKey::Tab(tab) => {
                state.select(tab);
            }
            ShellKey::Mark => {
                if marks.mark() {
                    log_view.notify("Repère posé dans le journal".to_owned(), ctx.input(|i| i.time));
                }
            }
        }
    }
}

/// Ce que le repère `M` lit : l'enregistreur (s'il est là, le repère rejoint aussi `events.jsonl`), le tampon du
/// journal, et l'horloge de simulation pour le libellé.
#[derive(SystemParam)]
pub(crate) struct MarkParams<'w> {
    recorder: Option<Res<'w, DebugLog>>,
    log: Option<Res<'w, OperatorLog>>,
    clock: Option<Res<'w, SimClock>>,
}

impl MarkParams<'_> {
    /// Pose le repère ; `false` s'il n'y a aucun journal à marquer.
    pub(crate) fn mark(&self) -> bool {
        let recorder = self.recorder.as_deref().map(|r| &r.0);
        let buffer = self.log.as_deref().map(|l| &l.0);
        if recorder.is_none() && buffer.is_none() {
            return false;
        }
        let label = self.clock.as_deref().map_or_else(
            || "repère opérateur".to_owned(),
            |c| format!("repère opérateur · sim {:.3} s", c.sim_s()),
        );
        logbuf::mark(recorder, buffer, &label);
        true
    }
}

/// Ce que la coque lit et pilote en plus du panneau principal.
#[derive(SystemParam)]
pub(crate) struct ShellParams<'w, 's> {
    pub state: ResMut<'w, ShellState>,
    pub sim: Option<ResMut<'w, SimControl>>,
    /// Horloge de simulation (onglet Debug).
    pub clock: Option<Res<'w, SimClock>>,
    /// Mode manuel du monde (onglet Debug).
    pub automation: Option<ResMut<'w, WorldAutomation>>,
    /// Registre des événements en cours (onglet Debug).
    pub active: Option<ResMut<'w, ActiveEvents>>,
    pub diagnostics: Option<Res<'w, DiagnosticsSnapshot>>,
    pub session: Option<ResMut<'w, MeasurementSession>>,
    pub time: Res<'w, Time<Real>>,
    /// État du panneau de diagnostic des statistiques (dernier export).
    pub stats: Local<'s, PanelState>,
}

/// Ce que l'en-tête a décidé pendant l'image.
#[derive(Debug, Default)]
pub(crate) struct HeaderOut {
    /// Onglet nouvellement choisi.
    pub picked: Option<Tab>,
    /// Clic sur « Installer » du badge de mise à jour.
    pub install: bool,
}

/// L'en-tête : les trois onglets à gauche, le badge de mise à jour puis `extra` (réglage de l'écran) à droite.
pub(crate) fn header(
    root: &mut egui::Ui,
    state: &ShellState,
    update: &UpdateStatus,
    show_running: bool,
    extra: impl FnOnce(&mut egui::Ui),
) -> HeaderOut {
    let mut out = HeaderOut::default();
    egui::Panel::top("shell_header").show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("cyber-funk").strong().size(17.0));
            ui.add_space(12.0);
            for tab in Tab::ALL {
                if tab_button(ui, tab, state.tab == tab) {
                    out.picked = Some(tab);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                extra(ui);
                ui.add_space(8.0);
                out.install = update_badge::draw(ui, update, show_running);
            });
        });
        ui.add_space(2.0);
    });
    out
}

/// Un onglet : bouton large, avec un trait de couleur sous l'onglet affiché. Rend vrai au clic.
fn tab_button(ui: &mut egui::Ui, tab: Tab, selected: bool) -> bool {
    let label = egui::RichText::new(tab.label()).size(16.0);
    let response = ui
        .add(egui::Button::selectable(selected, label).min_size(egui::vec2(88.0, 32.0)))
        .on_hover_text(shortcuts::describe(ShellKey::Tab(tab).text()));
    if selected {
        let rect = response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() + 1.0,
            egui::Stroke::new(3.0, ui.visuals().selection.stroke.color),
        );
    }
    response.clicked() && !selected
}
