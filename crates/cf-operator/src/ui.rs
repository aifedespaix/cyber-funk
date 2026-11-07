//! Interface egui de la fenêtre opérateur.

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};

use cf_render::audio::{AudioCommand, AudioFeatures, AudioStatus, VisualLatency};
use cf_render::{ControlsStatus, LearnCommand, ScreenList, ScreenOrder};

use crate::live::{self, LiveParams};
use crate::loading::{self, LoadingView};
use crate::prepare::PrepSession;
use crate::quit::{QuitDecision, QuitPrompt, QuitSource};
use crate::shell::{self, ShellParams, Tab};
use crate::tabs::debug::{self, DebugParams, Shared};
use crate::tabs::settings::screens_view::ScreenCard;
use crate::tabs::settings::update_panel::{self, InstallUpdate, UpdateStatus};
use crate::tabs::settings::{self as settings_tab, PrepParams, QuitChoice, audio_panel, controls_panel, learn_panel};
use crate::tabs::stats::{self, StatsParams};

/// Largeur initiale du panneau audio (points egui) : assez pour les vumètres et la ligne des compteurs.
const AUDIO_PANEL_WIDTH: f32 = 560.0;

/// Panneau de l'opérateur : l'en-tête de la coque (onglets + barre de statut), puis l'onglet affiché.
///
/// Sans `CfAudioPlugin`, les ressources audio manquent : le panneau le dit au lieu de s'afficher à vide.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
pub(crate) fn main_panel(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    status: Option<Res<AudioStatus>>,
    features: Option<Res<AudioFeatures>>,
    latency: Option<ResMut<VisualLatency>>,
    mut commands: MessageWriter<AudioCommand>,
    controls: Option<Res<ControlsStatus>>,
    loading: LoadingView,
    mut live: LiveParams,
    mut prep: PrepParams,
    mut learn: MessageWriter<LearnCommand>,
    update: Res<UpdateStatus>,
    mut install: MessageWriter<InstallUpdate>,
    mut debug: DebugParams,
    mut shell: ShellParams,
    mut stats: StatsParams,
    mut screens: ScreenParams,
) {
    // Depuis egui 0.36 les panneaux se posent dans un `Ui` racine, pas directement sur le contexte.
    let ctx = context.get_mut();
    let mut root = egui::Ui::new(
        ctx.clone(),
        "operator_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let mode = *live.mode;
    let data = shell.status_data(status.as_deref(), features.as_deref(), live.show_and_sheet());
    let mut state = shell.state.clone();
    state.refresh(data.now_s, data.slot_seq);
    let (picked, actions) = shell::header(&mut root, &mut state, mode, &data);
    if let Some(tab) = picked {
        state.select(tab, mode);
    }
    shell.state.set_if_neq(state);
    if !actions.is_empty() {
        shell.apply(&actions, &mut live.orders.0);
    }
    match shell.state.shown(mode) {
        Tab::Settings => {}
        Tab::Stats => {
            stats::show(&mut root, &mut shell, &mut stats, features.as_deref());
            return;
        }
        Tab::Debug => {
            let shared = Shared {
                automation: &mut shell.automation,
                active: &mut shell.active,
                clock: shell.clock.as_deref(),
            };
            debug::show(&mut root, &mut debug, shared, mode, &mut live.orders.0);
            return;
        }
    }
    // Onglet 1 en Live : la vue Live, intacte (édition verrouillée, rien d'autre que le pupitre sous l'en-tête).
    if !loading.is_loading() && live.is_live() {
        // Aucune confirmation de l'onglet Réglages ne survit au passage en Live.
        prep.drop_pending();
        live.draw(&mut root, features.as_deref(), controls.as_deref());
        return;
    }
    let features_copy = features.as_deref().copied();
    egui::Panel::right("audio_panel")
        .default_size(AUDIO_PANEL_WIDTH)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match (status, features, latency) {
                (Some(status), Some(features), Some(mut latency)) => {
                    let mut orders = Vec::new();
                    audio_panel::draw(ui, &status, &features, &mut latency, &mut orders);
                    commands.write_batch(orders);
                }
                _ => {
                    ui.weak("Audio non branché (CfAudioPlugin absent).");
                }
            });
        });
    // Avec une session de préparation, les contrôles passent dans un volet du bas repliable : le centre est à l'éditeur.
    if prep.session.is_some() && !loading.is_loading() {
        egui::Panel::bottom("controls_panel")
            .resizable(true)
            .show(&mut root, |ui| {
                egui::CollapsingHeader::new("Contrôles et MIDI")
                    .default_open(false)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            draw_controls(ui, live.show(), controls.as_deref(), features_copy);
                            if let Some(status) = controls.as_deref() {
                                ui.separator();
                                let mut orders = Vec::new();
                                learn_panel::draw(ui, status, &mut orders);
                                learn.write_batch(orders);
                            }
                        });
                    });
            });
    }
    egui::CentralPanel::default().show(&mut root, |ui| {
        // Pendant le chargement, la progression remplace le pupitre : rien à piloter tant que le show n'est pas prêt.
        if loading.is_loading() {
            loading::draw(ui, &loading);
            return;
        }
        loading::draw_late_warning(ui, loading.late_pipelines());
        // Hors vue Live seulement (elle a déjà rendu la main plus haut) : jamais de mise à jour proposée en show.
        if update_panel::draw(ui, &update) {
            install.write(InstallUpdate);
        }
        if let (Some(session), Some(tab)) = (prep.session.as_deref_mut(), prep.tab.as_deref_mut()) {
            let library_ids = controls.as_deref().map_or(&[][..], |c| c.library_ids.as_slice());
            let empty = ScreenList::default();
            let card = ScreenCard {
                list: screens.list.as_deref().unwrap_or(&empty),
                live: mode == crate::OperatorMode::Live,
                now_s: shell.time.elapsed_secs_f64(),
            };
            let mut orders = Vec::new();
            settings_tab::draw(ui, session, tab, library_ids, card, &mut orders, |ui| {
                live::mode_button(ui, &mut live);
            });
            screens.orders.write_batch(orders);
            return;
        }
        ui.horizontal(|ui| {
            ui.heading("Réglages");
            live::mode_button(ui, &mut live);
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            draw_controls(ui, live.show(), controls.as_deref(), features_copy);
        });
    });
}

/// La carte « Écran de sortie » : la liste écrite par le projecteur (binaire), et ses ordres.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScreenParams<'w> {
    list: Option<Res<'w, ScreenList>>,
    orders: MessageWriter<'w, ScreenOrder>,
}

/// Le panneau des contrôles, ou la raison de son absence.
fn draw_controls(
    ui: &mut egui::Ui,
    show: Option<&cf_show::director::ShowState>,
    controls: Option<&ControlsStatus>,
    features: Option<AudioFeatures>,
) {
    match (controls, features) {
        (Some(controls), Some(features)) => controls_panel::draw(ui, show, controls, &features),
        _ => {
            ui.weak("Contrôles non branchés (ControlsPlugin absent).");
        }
    }
}

/// Dialogue de confirmation de sortie, affiché tant qu'une fermeture est en attente.
///
/// `Échap` ferme le dialogue (annule) : c'est une sortie du dialogue, jamais de l'application. Avec une soirée
/// modifiée, le dialogue propose d'abord de l'enregistrer (profil actif) ; un enregistrement refusé garde le
/// dialogue ouvert avec la raison.
pub(crate) fn quit_dialog(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    prompt: Res<QuitPrompt>,
    mut decisions: MessageWriter<QuitDecision>,
    mut prep: PrepParams,
    mut settings: Option<ResMut<cf_render::Settings>>,
) {
    let Some(source) = prompt.pending else {
        return;
    };
    let ctx = context.get_mut();
    let consequence = match source {
        QuitSource::Operator => "Fermer l'opérateur arrête aussi le projecteur.",
        QuitSource::Projector => "Fermer le projecteur arrête aussi l'opérateur.",
    };
    let dirty = prep.session.as_deref().is_some_and(PrepSession::is_dirty);
    let mut picked = None;
    egui::Window::new("Quitter cyber-funk ?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(consequence);
            if let Some(session) = prep.session.as_deref().filter(|_| dirty) {
                ui.colored_label(egui::Color32::YELLOW, "La soirée a des modifications non enregistrées.");
                if let crate::prepare::Status::Error(message) = &session.status {
                    ui.colored_label(egui::Color32::LIGHT_RED, message);
                }
            }
            ui.horizontal(|ui| {
                for (choice, label) in settings_tab::quit_choices(dirty) {
                    if ui.button(*label).clicked() {
                        picked = Some(*choice);
                    }
                }
            });
        });
    let confirmed = match picked {
        Some(QuitChoice::Quit | QuitChoice::QuitWithoutSaving) => true,
        Some(QuitChoice::SaveAndQuit) => match (prep.session.as_deref_mut(), prep.tab.as_deref_mut()) {
            (Some(session), Some(tab)) => settings_tab::profiles::save_evening(session, tab),
            _ => true,
        },
        Some(QuitChoice::Cancel) => {
            decisions.write(QuitDecision::Cancel);
            false
        }
        None => false,
    };
    if confirmed {
        if let Some(settings) = settings.as_deref_mut() {
            settings_tab::restore::flush_on_quit(settings, prep.tab.as_deref_mut());
        }
        decisions.write(QuitDecision::Confirm);
    }

    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        decisions.write(QuitDecision::Cancel);
    }
}
