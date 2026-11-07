//! La coque de la fenêtre opérateur : barre d'onglets, barre de statut permanente, raccourcis globaux et aide `?`.
//!
//! ```text
//! EguiPrimaryContextPass :
//!   shell_input   Ctrl+1/2/3 (onglets), ? (aide), Échap (Noir)  ─▶ ShellState, LiveOrders
//!   main_panel    en-tête (onglets + barre de statut) puis l'onglet affiché
//!   persist_tab   ShellState::tab ─▶ Settings.value.operator.last_tab (seulement s'il a changé)
//! ```
//!
//! En Live, l'onglet 1 est la vue Live (intacte), l'onglet 2 les statistiques ; l'onglet Debug n'est ni affiché ni
//! accessible au clavier (un clic de trop ne doit pas lancer un événement devant le public). L'onglet choisi est gardé
//! tel quel : on le retrouve en revenant en Préparation.

mod help;
pub(crate) mod shortcuts;
pub(crate) mod status_bar;
#[cfg(test)]
mod tests;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_control::{Action, Switch};
use cf_render::WorldAutomation;
use cf_render::audio::{AudioFeatures, AudioStatus};
use cf_render::{ActiveEvents, DiagnosticsSnapshot, MeasurementSession, Settings, ShowSheet, SimClock, SimControl};
use cf_show::director::ShowState;

use crate::OperatorMode;
use crate::live::{LiveOrders, ModePrompt};
use crate::quit::QuitPrompt;
use crate::tabs::debug::debug_panel::DebugState;
use crate::tabs::stats::diagnostics::PanelState;
use shortcuts::{KeyGuard, ShellKey};
use status_bar::{StatusAction, StatusData};

/// Les onglets de la fenêtre opérateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tab {
    /// Réglages en Préparation ; la vue Live en Live.
    #[default]
    Settings,
    /// Statistiques (diagnostic).
    Stats,
    /// Debug du monde (Préparation seulement).
    Debug,
}

impl Tab {
    /// Les onglets, dans l'ordre de la barre.
    pub(crate) const ALL: [Tab; 3] = [Self::Settings, Self::Stats, Self::Debug];

    /// Identifiant stable, écrit dans `OperatorPrefs::last_tab` : ne jamais le renommer.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Stats => "stats",
            Self::Debug => "debug",
        }
    }

    /// L'onglet de cet identifiant, `None` s'il est inconnu.
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.key() == key)
    }

    /// L'onglet à rouvrir au démarrage : celui retenu, ou Réglages si la valeur est inconnue (fichier d'une autre
    /// version, édité à la main).
    pub(crate) fn restore(key: &str) -> Self {
        Self::from_key(key).unwrap_or_default()
    }

    /// L'onglet existe-t-il dans ce mode ? Debug est retiré du Live.
    pub(crate) fn visible_in(self, mode: OperatorMode) -> bool {
        !(self == Self::Debug && mode == OperatorMode::Live)
    }

    /// Libellé de la barre d'onglets.
    pub(crate) fn label(self, mode: OperatorMode) -> &'static str {
        match (self, mode) {
            (Self::Settings, OperatorMode::Live) => "Live",
            (Self::Settings, OperatorMode::Preparation) => "Réglages",
            (Self::Stats, _) => "Statistiques",
            (Self::Debug, _) => "Debug",
        }
    }
}

/// L'état de la coque (jamais sauvegardé, sauf l'onglet par `persist_tab`).
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ShellState {
    /// Onglet choisi par l'opérateur.
    pub tab: Tab,
    /// L'aide des raccourcis est ouverte.
    pub help_open: bool,
    /// « Passer au suivant » attend sa confirmation.
    pub confirm_next_dj: bool,
    /// Le reset de la simulation attend sa confirmation.
    pub confirm_reset: bool,
}

/// Un geste en deux temps : le premier appel arme (`false`), le second confirme (`true`) et désarme.
fn two_step(armed: &mut bool) -> bool {
    *armed = !*armed;
    !*armed
}

impl ShellState {
    /// L'onglet effectivement affiché dans ce mode : celui choisi, ou Réglages/Live s'il n'existe pas dans ce mode.
    pub(crate) fn shown(&self, mode: OperatorMode) -> Tab {
        if self.tab.visible_in(mode) {
            self.tab
        } else {
            Tab::Settings
        }
    }

    /// Choisit un onglet. `false` si rien ne change (déjà affiché, ou absent de ce mode).
    pub(crate) fn select(&mut self, tab: Tab, mode: OperatorMode) -> bool {
        if !tab.visible_in(mode) || self.shown(mode) == tab {
            return false;
        }
        self.tab = tab;
        true
    }

    /// Clic sur « Passer au suivant » ou sur sa confirmation : `true` quand le changement de DJ est confirmé.
    pub(crate) fn click_next_dj(&mut self) -> bool {
        two_step(&mut self.confirm_next_dj)
    }

    /// Clic sur « Reset » ou sur sa confirmation : `true` quand le reset est confirmé.
    pub(crate) fn click_reset(&mut self) -> bool {
        two_step(&mut self.confirm_reset)
    }
}

/// Branche la coque. Ajouté par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.init_resource::<ShellState>().add_systems(Startup, restore_tab);
}

/// Rouvre l'onglet de la dernière session (et la fenêtre Debug monde avec l'onglet Debug, comme un clic l'aurait fait).
fn restore_tab(settings: Option<Res<Settings>>, mut state: ResMut<ShellState>, debug: Option<ResMut<DebugState>>) {
    if let Some(settings) = settings {
        state.tab = Tab::restore(&settings.value.operator.last_tab);
    }
    if state.tab == Tab::Debug
        && let Some(mut debug) = debug
    {
        debug.open = true;
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

/// Les touches globales. Rien pendant une saisie de texte ; `Échap` reste au dialogue ouvert (il annule).
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
pub(crate) fn shell_input(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    mode: Res<OperatorMode>,
    prompt: Res<ModePrompt>,
    quit: Option<Res<QuitPrompt>>,
    mut state: ResMut<ShellState>,
    mut orders: ResMut<LiveOrders>,
    mut debug: ResMut<DebugState>,
    mut pressed: Local<Vec<ShellKey>>,
) {
    let ctx = context.get_mut();
    let guard = KeyGuard {
        typing: ctx.egui_wants_keyboard_input(),
        dialog_open: prompt.target.is_some() || quit.is_some_and(|q| q.pending.is_some()),
    };
    pressed.clear();
    ctx.input(|input| shortcuts::poll(input, *mode, guard, &mut pressed));
    for key in pressed.drain(..) {
        match key {
            ShellKey::Tab(tab) => {
                if state.select(tab, *mode) && tab == Tab::Debug {
                    debug.open = true;
                }
            }
            ShellKey::Help => state.help_open = !state.help_open,
            ShellKey::Blackout => orders.0.push(Action::Blackout(Switch::Toggle)),
        }
    }
}

/// Ce que la coque lit et pilote en plus du panneau principal.
#[derive(SystemParam)]
pub(crate) struct ShellParams<'w, 's> {
    pub state: ResMut<'w, ShellState>,
    sim: Option<ResMut<'w, SimControl>>,
    clock: Option<Res<'w, SimClock>>,
    automation: Option<ResMut<'w, WorldAutomation>>,
    active: Option<Res<'w, ActiveEvents>>,
    pub diagnostics: Option<Res<'w, DiagnosticsSnapshot>>,
    pub session: Option<ResMut<'w, MeasurementSession>>,
    pub time: Res<'w, Time<Real>>,
    /// État du panneau de diagnostic de l'onglet Statistiques (dernier export).
    pub stats: Local<'s, PanelState>,
}

/// Le spectacle de l'image, s'il y en a un.
pub(crate) type ShowView<'a> = Option<(&'a ShowState, &'a ShowSheet)>;

impl ShellParams<'_, '_> {
    /// Les valeurs de la barre de statut. Une valeur absente (plugin non monté) reste `None` : la barre affiche « — ».
    pub(crate) fn status_data<'a>(
        &'a self,
        audio: Option<&AudioStatus>,
        features: Option<&AudioFeatures>,
        show: ShowView<'a>,
    ) -> StatusData<'a> {
        let slot = show.and_then(|(state, sheet)| sheet.slot(state.slot).map(|s| (state, s)));
        StatusData {
            audio: audio.map(|a| (a.source.label(), a.analysis_running)),
            fps: self.diagnostics.as_deref().map(|d| d.fps),
            bpm: features.and_then(|f| f.bpm),
            dj: slot.map(|(state, s)| (s.dj.as_str(), state.slot)),
            remaining_s: slot.and_then(|(state, s)| status_bar::remaining_s(s.schedule, state.elapsed)),
            active_events: self.active.as_deref().map(ActiveEvents::len),
            manual: self.automation.as_deref().map(|a| a.manual),
            paused: self.sim.as_deref().map(SimControl::paused),
            clock: self.clock.as_deref().map(|c| (c.sim_ms(), c.frame())),
            has_show: show.is_some(),
            blackout_on: show.is_some_and(|(state, _)| state.blackout_on),
        }
    }

    /// Applique les gestes de la barre. Noir et DJ suivant passent par `LiveOrders`, le chemin de la vue Live.
    pub(crate) fn apply(&mut self, actions: &[StatusAction], orders: &mut Vec<Action>) {
        for action in actions {
            match action {
                StatusAction::Blackout => orders.push(Action::Blackout(Switch::Toggle)),
                StatusAction::NextDj => orders.push(Action::NextDj),
                StatusAction::TogglePause => {
                    if let Some(sim) = self.sim.as_mut() {
                        sim.toggle_pause();
                    }
                }
                StatusAction::ConfirmReset => {
                    if let Some(sim) = self.sim.as_mut() {
                        sim.request_reset();
                    }
                }
                StatusAction::ToggleManual => {
                    if let Some(automation) = self.automation.as_mut() {
                        automation.manual = !automation.manual;
                    }
                }
            }
        }
    }
}

/// L'en-tête : barre d'onglets puis barre de statut, en haut de la fenêtre dans tous les onglets et en Live. Rend
/// l'onglet nouvellement choisi, s'il y en a un.
pub(crate) fn header(
    root: &mut egui::Ui,
    state: &mut ShellState,
    mode: OperatorMode,
    data: &StatusData<'_>,
) -> (Option<Tab>, Vec<StatusAction>) {
    let mut picked = None;
    let mut actions = Vec::new();
    egui::Panel::top("shell_header").show(root, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("cyber-funk");
            ui.separator();
            for tab in Tab::ALL.into_iter().filter(|t| t.visible_in(mode)) {
                let key = shortcut_of(tab);
                let label = format!("{}  [{key}]", tab.label(mode));
                let selected = state.shown(mode) == tab;
                if ui
                    .add(egui::Button::selectable(selected, label))
                    .on_hover_text(shortcuts::describe(key))
                    .clicked()
                {
                    picked = Some(tab);
                }
            }
            ui.separator();
            if ui
                .button(format!("Aide  [{}]", shortcuts::KEY_HELP))
                .on_hover_text(shortcuts::describe(shortcuts::KEY_HELP))
                .clicked()
            {
                state.help_open = !state.help_open;
            }
        });
        ui.separator();
        actions = status_bar::draw(ui, data, state);
    });
    help::window(root.ctx(), &mut state.help_open);
    (picked, actions)
}

fn shortcut_of(tab: Tab) -> &'static str {
    ShellKey::Tab(tab).text()
}
