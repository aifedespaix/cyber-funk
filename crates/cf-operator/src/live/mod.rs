//! La vue Live de la fenêtre opérateur (J8-01) : piloter une soirée de bout en bout, édition verrouillée.
//!
//! ```text
//! EguiPrimaryContextPass (fenêtre opérateur, jamais le projecteur) :
//!   live_input      touches propres à la vue, maintien du Blackout, relâchements forcés (changement de DJ / profil)
//!   main_panel      la vue (ou, en Préparation, les panneaux d'avant) : écrit les clics dans `LiveOrders`
//!   mode_dialog     confirmation du changement Préparation / Live
//!   flush_orders    `LiveOrders` ─▶ `ControlAction` (même chemin que le clavier et le MIDI : `cf-render`)
//! ```
//!
//! Aucune action de cette vue ne contourne le limiteur photosensible : elles deviennent des `Action` que lit le chef
//! du spectacle ; la luminance finale passe par le limiteur de `cf-show` quoi qu'il arrive (règle 7).

mod bottom;
mod commands;
mod holds;
mod side;
mod text;
mod view;
mod widgets;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_control::{Action, TimedAction};
use cf_render::audio::AudioFeatures;
use cf_render::{
    ControlAction, ControlBridge, ControlsStatus, CurrentShow, DiagnosticsSnapshot, ShowEventMessage, ShowRuntime,
    ShowSheet, submit_action,
};
use cf_show::director::{ShowEvent, ShowState};

use commands::LiveCommand;
use holds::HoldTracker;
use view::LiveData;

/// Le mode de la fenêtre opérateur.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatorMode {
    /// Préparation : panneaux de réglage, édition permise.
    #[default]
    Preparation,
    /// Live : édition verrouillée, gros éléments lisibles dans le noir.
    Live,
}

impl OperatorMode {
    /// L'autre mode.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Preparation => Self::Live,
            Self::Live => Self::Preparation,
        }
    }
}

/// Demande de changement de mode en attente de confirmation : la bascule est protégée (un clic perdu en Live ne doit
/// pas déverrouiller l'édition pendant que le show joue).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModePrompt {
    pub target: Option<OperatorMode>,
}

/// Les ordres de l'image, remplis par la vue et les touches, envoyés par `flush_orders`. Le tampon est réutilisé.
#[derive(Resource, Debug, Default)]
pub(crate) struct LiveOrders(pub Vec<Action>);

/// Les boutons à maintien de la vue.
#[derive(Resource, Debug, Default)]
pub(crate) struct LiveHolds {
    blackout: HoldTracker,
}

/// Branche la vue Live. Ajouté par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.add_message::<ControlAction>()
        .add_message::<ShowEventMessage>()
        .init_resource::<OperatorMode>()
        .init_resource::<ModePrompt>()
        .init_resource::<LiveOrders>()
        .init_resource::<LiveHolds>()
        .add_systems(Startup, open_in_live_when_a_show_is_loaded);
}

/// Avec un show chargé on ouvre en Live (c'est ce qu'on lance en soirée) ; sans show il n'y a rien à piloter.
fn open_in_live_when_a_show_is_loaded(show: Option<Res<CurrentShow>>, mut mode: ResMut<OperatorMode>) {
    if show.is_some() {
        *mode = OperatorMode::Live;
    }
}

/// Les ressources que la vue lit en plus de celles du panneau principal.
#[derive(SystemParam)]
pub(crate) struct LiveParams<'w> {
    pub mode: Res<'w, OperatorMode>,
    pub prompt: ResMut<'w, ModePrompt>,
    pub orders: ResMut<'w, LiveOrders>,
    show: Option<Res<'w, CurrentShow>>,
    sheet: Option<Res<'w, ShowSheet>>,
    runtime: Option<Res<'w, ShowRuntime>>,
    diagnostics: Option<Res<'w, DiagnosticsSnapshot>>,
}

impl LiveParams<'_> {
    /// La vue Live est-elle celle à afficher ? Il lui faut un show et sa feuille de conduite.
    pub(crate) fn is_live(&self) -> bool {
        *self.mode == OperatorMode::Live && self.show.is_some() && self.sheet.is_some()
    }

    /// L'état du spectacle de l'image, s'il y a un show.
    pub(crate) fn show(&self) -> Option<&ShowState> {
        self.show.as_deref().map(|s| &s.0)
    }

    /// Dessine la vue Live. Ne fait rien sans show.
    pub(crate) fn draw(
        &mut self,
        root: &mut egui::Ui,
        features: Option<&AudioFeatures>,
        controls: Option<&ControlsStatus>,
    ) {
        let (Some(show), Some(sheet)) = (self.show.as_deref(), self.sheet.as_deref()) else {
            return;
        };
        let data = LiveData {
            state: &show.0,
            sheet,
            features,
            controls,
            diagnostics: self.diagnostics.as_deref(),
            drop_plan: self.runtime.as_deref().and_then(|r| r.director().drop_plan()),
        };
        let mut mode_request = false;
        view::draw(root, &data, &mut self.orders.0, &mut mode_request);
        if mode_request {
            self.prompt.target = Some(self.mode.other());
        }
    }
}

/// Bouton « Passer en Live » de la Préparation (la vue Live a le sien dans sa frise).
pub(crate) fn mode_button(ui: &mut egui::Ui, params: &mut LiveParams<'_>) {
    let has_show = params.show.is_some();
    let command = LiveCommand::SwitchMode;
    let label = format!("Passer en Live  [{}]", command.shortcut_text());
    if ui.add_enabled(has_show, egui::Button::new(label)).clicked() {
        params.prompt.target = Some(OperatorMode::Live);
    }
    if !has_show {
        ui.weak("Pas de show chargé : rien à piloter en Live.");
    }
}

/// Le show et sa feuille de conduite, optionnels : sans show il n'y a rien à piloter.
#[derive(SystemParam)]
pub(crate) struct ShowRefs<'w> {
    show: Option<Res<'w, CurrentShow>>,
    sheet: Option<Res<'w, ShowSheet>>,
}

/// Les touches propres à la vue, le maintien du Blackout et les relâchements forcés.
///
/// Un changement de DJ ou de profil de contrôle (`SlotLoaded`, début de passation) ou une sortie du mode Live relâche
/// tout maintien tout de suite (voir [`HoldTracker`]).
pub(crate) fn live_input(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    mode: Res<OperatorMode>,
    mut prompt: ResMut<ModePrompt>,
    mut holds: ResMut<LiveHolds>,
    mut orders: ResMut<LiveOrders>,
    show: ShowRefs,
    mut facts: MessageReader<ShowEventMessage>,
    mut pressed: Local<Vec<LiveCommand>>,
) {
    let ctx = context.get_mut();
    let blackout_key = ctx.input(|i| commands::key_down(i, LiveCommand::Blackout));
    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let typing = ctx.egui_wants_keyboard_input();
    let live = *mode == OperatorMode::Live;

    // Relâchements forcés : le contexte du maintien disparaît.
    let handover = facts
        .read()
        .any(|f| matches!(f.0, ShowEvent::SlotLoaded { .. } | ShowEvent::Handover { .. }));
    if (handover || (mode.is_changed() && !live))
        && let Some(switch) = holds.blackout.force_release(blackout_key)
    {
        orders.0.push(Action::Blackout(switch));
    }

    let dialog_open = prompt.target.is_some();
    let allowed = live && focused && !typing && !dialog_open;
    let blackout_on = show.show.as_deref().is_some_and(|s| s.blackout_on);
    if let Some(switch) = holds.blackout.update(blackout_key, allowed, blackout_on) {
        orders.0.push(Action::Blackout(switch));
    }

    pressed.clear();
    if focused && !typing && !dialog_open {
        ctx.input(|i| commands::poll(i, &mut pressed));
    }
    let mut request = None;
    let context = show.show.as_deref().zip(show.sheet.as_deref()).map(|(show, sheet)| {
        let data = LiveData {
            state: &show.0,
            sheet,
            features: None,
            controls: None,
            diagnostics: None,
            drop_plan: None,
        };
        data.command_context()
    });
    for command in pressed.drain(..) {
        if command == LiveCommand::SwitchMode {
            request = Some(mode.other());
        } else if live && let Some(context) = &context {
            orders.0.extend(command.action(context));
        }
    }
    if let Some(target) = request {
        // Sans show, la Préparation n'a pas de Live vers lequel aller.
        if target == OperatorMode::Preparation || show.show.is_some() {
            prompt.target = Some(target);
        }
    }
}

/// Dialogue de confirmation du changement de mode. Il faut un clic explicite : ni Entrée ni la touche de bascule ne
/// confirment, `Échap` annule.
pub(crate) fn mode_dialog(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    mut prompt: ResMut<ModePrompt>,
    mut mode: ResMut<OperatorMode>,
) {
    let Some(target) = prompt.target else {
        return;
    };
    let ctx = context.get_mut();
    let (title, warning, confirm, cancel) = match target {
        OperatorMode::Preparation => (
            "Quitter le mode Live ?",
            "L'édition sera déverrouillée alors que le show continue de jouer : une fausse manœuvre peut le modifier.",
            "Déverrouiller (Préparation)",
            "Rester en Live",
        ),
        OperatorMode::Live => (
            "Passer en Live ?",
            "L'édition sera verrouillée. Les raccourcis de la vue Live deviennent actifs.",
            "Verrouiller (Live)",
            "Rester en Préparation",
        ),
    };
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(warning);
            ui.horizontal(|ui| {
                if ui.button(confirm).clicked() {
                    *mode = target;
                    prompt.target = None;
                }
                if ui.button(cancel).clicked() {
                    prompt.target = None;
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        prompt.target = None;
    }
}

/// Envoie les ordres de l'image au spectacle, par le chemin des contrôles quand il existe (le routage du tap tempo et
/// la « dernière action » du panneau en dépendent), sinon directement.
pub(crate) fn flush_orders(
    mut orders: ResMut<LiveOrders>,
    bridge: Option<ResMut<ControlBridge>>,
    status: Option<ResMut<ControlsStatus>>,
    mut out: MessageWriter<ControlAction>,
    time: Res<Time<Real>>,
) {
    if orders.0.is_empty() {
        return;
    }
    if let (Some(mut bridge), Some(mut status)) = (bridge, status) {
        for action in orders.0.drain(..) {
            submit_action(&mut bridge, &mut status, &mut out, action);
        }
    } else {
        let time_s = time.elapsed_secs_f64();
        for action in orders.0.drain(..) {
            out.write(ControlAction(TimedAction { time_s, action }));
        }
    }
}

#[cfg(test)]
mod tests;
