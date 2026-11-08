//! Interface egui de la fenêtre opérateur : en-tête (onglets, badge de mise à jour), puis l'onglet affiché.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};

use cf_control::{Action, Switch};
use cf_render::audio::{AudioCommand, AudioFeatures, AudioStatus};
use cf_render::{ControlsStatus, CurrentShow, MidiEnabled, ScreenList, ScreenOrder, ShowSheet, ThemeLibrary};

use crate::OperatorMode;
use crate::cockpit::center::LiveStats;
use crate::cockpit::dj_column::{self, DjCard, DjColumnEvent};
use crate::cockpit::files::FilePicker;
use crate::cockpit::logos::LogoCache;
use crate::cockpit::playlist_view::{self, PlaylistUi};
use crate::cockpit::view::{self as live_view, LiveInput, LiveOut};
use crate::format;
use crate::loading::{self, LoadingView};
use crate::orders::LiveOrders;
use crate::prepare::{PrepConfig, PrepSession};
use crate::quit::{QuitDecision, QuitPrompt, QuitSource};
use crate::run_state::{DjPick, ShowRun, ShowRunState};
use crate::shell::{self, DebugView, ShellParams, Tab};
use crate::tabs::debug::{self, DebugParams, Shared};
use crate::tabs::settings::screens_view::ScreenCard;
use crate::tabs::settings::{self as settings_tab, PrepParams, QuitChoice};
use crate::tabs::stats::{self, StatsParams};

/// L'état propre à l'onglet Live, jamais sauvegardé.
#[derive(Resource, Debug, Default)]
pub(crate) struct CockpitState {
    /// Sélection de la playlist.
    pub playlist: PlaylistUi,
    /// « + Nouveau DJ » a été demandé : l'onglet DJs ajoute un DJ à son prochain affichage.
    pub new_dj: bool,
    /// Les listes de périphériques ont été demandées une première fois.
    pub devices_requested: bool,
}

/// Tout ce que l'onglet Live lit et écrit en plus du reste.
#[derive(SystemParam)]
pub(crate) struct CockpitParams<'w> {
    run: ResMut<'w, ShowRunState>,
    state: ResMut<'w, CockpitState>,
    logos: ResMut<'w, LogoCache>,
    picker: Res<'w, FilePicker>,
    orders: ResMut<'w, LiveOrders>,
    run_orders: MessageWriter<'w, cf_render::RunOrder>,
    midi: Option<ResMut<'w, MidiEnabled>>,
    themes: Option<Res<'w, ThemeLibrary>>,
    config: Res<'w, PrepConfig>,
    show: Option<Res<'w, CurrentShow>>,
    sheet: Option<Res<'w, ShowSheet>>,
    start_slot: Option<ResMut<'w, cf_render::StartSlot>>,
}

/// La carte « Écran de sortie » : la liste écrite par le projecteur (binaire), et ses ordres.
#[derive(SystemParam)]
pub(crate) struct ScreenParams<'w> {
    list: Option<Res<'w, ScreenList>>,
    orders: MessageWriter<'w, ScreenOrder>,
}

/// Panneau de l'opérateur : l'en-tête (onglets + badge), puis l'onglet affiché.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
pub(crate) fn main_panel(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    status: Option<Res<AudioStatus>>,
    features: Option<Res<AudioFeatures>>,
    mut commands: MessageWriter<AudioCommand>,
    controls: Option<Res<ControlsStatus>>,
    loading: LoadingView,
    mut cockpit: CockpitParams,
    mut prep: PrepParams,
    update: Res<shell::UpdateStatus>,
    mut install: MessageWriter<shell::InstallUpdate>,
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
    let now_s = shell.time.elapsed_secs_f64();
    let show_running = cockpit.run.is_active();

    // Les listes de périphériques se lisent une fois, au premier affichage (jamais à chaque image).
    if !cockpit.state.devices_requested {
        cockpit.state.devices_requested = true;
        commands.write(AudioCommand::RefreshDevices);
        commands.write(AudioCommand::RefreshOutputs);
    }
    // Des pistes choisies dans le sélecteur de fichiers arrivent à n'importe quel moment, quel que soit l'onglet.
    if let (Some(added), Some(status)) = (cockpit.picker.poll(), status.as_deref()) {
        commands.write(AudioCommand::SetPlaylist(playlist_view::with_added(
            &status.tracks,
            added,
        )));
    }

    let mut screen_orders = Vec::new();
    let header = {
        let screen_list = screens.list.as_deref().cloned().unwrap_or_default();
        let card = ScreenCard {
            list: &screen_list,
            live: show_running,
            now_s,
        };
        let tab = prep.tab.as_deref_mut();
        shell::header(&mut root, &shell.state, &update, show_running, |ui| {
            if let Some(tab) = tab {
                ui.menu_button("⚙ Écran de rendu", |ui| {
                    ui.set_min_width(300.0);
                    settings_tab::screens_view::draw(ui, card, &mut tab.screens, &mut screen_orders);
                });
            }
        })
    };
    screens.orders.write_batch(screen_orders);
    if let Some(tab) = header.picked {
        shell.state.select(tab);
    }
    if header.install {
        install.write(shell::InstallUpdate);
    }

    if loading.is_loading() {
        egui::CentralPanel::default().show(&mut root, |ui| loading::draw(ui, &loading));
        return;
    }
    if loading.late_pipelines() > 0 {
        egui::Panel::top("late_pipelines_banner").show(&mut root, |ui| {
            loading::draw_late_warning(ui, loading.late_pipelines());
        });
    }

    match shell.state.tab {
        Tab::Live => live_tab(
            &mut root,
            &mut cockpit,
            status.as_deref(),
            features.as_deref(),
            &mut shell,
            &mut commands,
            now_s,
        ),
        Tab::Djs => {
            let add_dj = std::mem::take(&mut cockpit.state.new_dj);
            egui::CentralPanel::default().show(&mut root, |ui| {
                match (prep.session.as_deref_mut(), prep.tab.as_deref_mut()) {
                    (Some(session), Some(tab)) => {
                        let library_ids = controls.as_deref().map_or(&[][..], |c| c.library_ids.as_slice());
                        settings_tab::draw(ui, session, tab, library_ids, add_dj);
                    }
                    _ => {
                        ui.weak("Aucune soirée chargée.");
                    }
                }
            });
        }
        Tab::Debug => {
            egui::Panel::top("debug_views").show(&mut root, |ui| {
                ui.horizontal(|ui| {
                    for (view, label) in [(DebugView::World, "Monde"), (DebugView::Stats, "Statistiques")] {
                        if ui
                            .add(egui::Button::selectable(shell.state.debug_view == view, label))
                            .clicked()
                        {
                            shell.state.debug_view = view;
                        }
                    }
                });
            });
            match shell.state.debug_view {
                DebugView::World => {
                    let shared = Shared {
                        automation: &mut shell.automation,
                        active: &mut shell.active,
                        clock: shell.clock.as_deref(),
                    };
                    debug::show(
                        &mut root,
                        &mut debug,
                        shared,
                        OperatorMode::of(&cockpit.run),
                        &mut cockpit.orders.0,
                    );
                }
                DebugView::Stats => stats::show(&mut root, &mut shell, &mut stats, features.as_deref()),
            }
        }
    }
}

/// L'onglet Live : construit ce que la vue lit, la dessine, puis exécute ce que l'opérateur a demandé.
fn live_tab(
    root: &mut egui::Ui,
    cockpit: &mut CockpitParams,
    status: Option<&AudioStatus>,
    features: Option<&AudioFeatures>,
    shell: &mut ShellParams,
    commands: &mut MessageWriter<AudioCommand>,
    now_s: f64,
) {
    let ctx = root.ctx().clone();
    let cards: Vec<DjCard> = match cockpit.sheet.as_deref().cloned() {
        Some(sheet) => sheet
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| DjCard {
                name: format::dj_name(&slot.dj, index),
                color: cockpit
                    .themes
                    .as_deref()
                    .map_or(egui::Color32::from_gray(120), |themes| {
                        dj_column::theme_color(themes, slot.theme.as_ref())
                    }),
                logo: slot
                    .logo
                    .as_deref()
                    .and_then(|logo| cockpit.logos.texture(&ctx, &cockpit.config.assets_dir, logo)),
            })
            .collect(),
        None => Vec::new(),
    };
    // À l'arrêt, le DJ « actuel » est celui que l'opérateur a désigné pour le prochain départ.
    let designated = cockpit.start_slot.as_deref().and_then(|start| start.0);
    let active_dj = match (cockpit.run.run, designated) {
        (ShowRun::Waiting, Some(slot)) if cockpit.run.is_sequenced() => Some(slot),
        _ => cockpit.show.as_deref().map(|show| show.slot),
    };
    let has_next = active_dj.is_some_and(|slot| slot + 1 < cards.len());
    let dj_name = active_dj
        .and_then(|slot| cards.get(slot))
        .map(|card| card.name.as_str());
    let stats = LiveStats {
        bpm: features.and_then(|f| f.bpm),
        section: features.map(|f| f.section).unwrap_or_default(),
        fps: shell.diagnostics.as_deref().map(|d| d.fps),
        level: features.map_or(0.0, |f| f.loud),
        live: features.is_some_and(|f| f.live),
    };
    let midi = cockpit.midi.as_deref().is_none_or(|midi| midi.0);
    let input = LiveInput {
        cards: &cards,
        active_dj,
        dj_name,
        run: &cockpit.run,
        elapsed_s: cockpit.run.elapsed_s(now_s),
        stats,
        audio: status,
        midi,
        blackout: cockpit.show.as_deref().is_some_and(|show| show.0.blackout_on),
        has_next,
    };
    let LiveOut {
        dj,
        press,
        noir,
        next,
        audio,
        midi: midi_change,
        add_files,
    } = live_view::draw(root, &input, &mut cockpit.state.playlist);

    match dj {
        DjColumnEvent::Pick(slot) => match cockpit.run.pick(slot) {
            DjPick::Order(order) => {
                cockpit.run_orders.write(order);
            }
            DjPick::StartAt(slot) => {
                if let Some(start) = cockpit.start_slot.as_mut() {
                    start.0 = Some(slot);
                }
            }
            DjPick::Direct(slot) => cockpit.orders.go_to_dj(slot),
            DjPick::DirectNext | DjPick::Ignore => {}
        },
        DjColumnEvent::New => {
            cockpit.state.new_dj = true;
            shell.state.select(Tab::Djs);
        }
        DjColumnEvent::None => {}
    }
    if press && let Some(order) = cockpit.run.press(now_s) {
        cockpit.run_orders.write(order);
    }
    if next && let Some(pick) = cockpit.run.next_dj(has_next) {
        match pick {
            DjPick::Order(order) => {
                cockpit.run_orders.write(order);
            }
            DjPick::DirectNext => {
                if let Some(slot) = active_dj {
                    cockpit.orders.go_to_dj(slot + 1);
                }
            }
            DjPick::StartAt(_) | DjPick::Direct(_) | DjPick::Ignore => {}
        }
    }
    if noir {
        cockpit.orders.0.push(Action::Blackout(Switch::Toggle));
    }
    commands.write_batch(audio);
    if let (Some(enabled), Some(midi)) = (midi_change, cockpit.midi.as_mut()) {
        midi.0 = enabled;
    }
    if add_files {
        cockpit.picker.open();
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
