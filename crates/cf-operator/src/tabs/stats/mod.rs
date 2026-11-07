//! Onglet Statistiques : quatre tuiles (images/s, tempo, événements joués, limiteur), quatre cartes (énergie,
//! spectre, événements par type, temps d'antenne), le diagnostic détaillé repliable, puis le journal de session en
//! direct dans toute la place restante.
//!
//! ```text
//! Update (CfSet::Present)  history : ActiveEvents, AudioFeatures, CurrentShow ─▶ EventTally, EnergyHistory, Airtime
//! main_panel               show : une copie du tampon du journal par image, filtrée dans un tampon réutilisé
//! shell_input              M : repère « mark » dans le journal (voir `services::logbuf::mark`)
//! ```
//!
//! Disponible en Live : l'onglet ne lance rien (le journal, le repère et les copies ne touchent pas au spectacle).

mod charts;
mod context;
mod copy;
pub(crate) mod diagnostics;
mod history;
pub(crate) mod log_view;
#[cfg(test)]
mod tests;
mod tiles;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use cf_debuglog::LogBuffer;
use cf_render::audio::AudioFeatures;
use cf_render::{
    CurrentShow, DebugLog, DiagnosticsSnapshot, DistrictState, MeasurementSession, PostFxState, ShowSheet,
};

use crate::OperatorMode;
use crate::services::logbuf::{self, OperatorLog};
use crate::shell::{MarkParams, ShellParams};
use charts::ChartData;
use context::{ContextSnapshot, Sources};
use diagnostics::PanelState;
use history::{Airtime, EnergyHistory, EventTally};
pub(crate) use log_view::LogViewState;
use tiles::TileData;

/// Part de la hauteur (sous les tuiles) donnée aux cartes.
const CARD_SHARE: f32 = 0.3;
/// Hauteur maximale des cartes (points egui) : le journal garde la plus grande part.
const CARD_MAX_H: f32 = 170.0;
/// En dessous, les cartes sont masquées : elles seraient illisibles et prendraient la place du journal.
const CARD_MIN_H: f32 = 56.0;
/// Place réservée au journal (barre d'outils sur deux ou trois rangées et quelques lignes) avant de dimensionner les
/// cartes.
const LOG_RESERVE_H: f32 = 150.0;
/// Hauteur maximale du diagnostic détaillé déplié.
const DETAIL_MAX_H: f32 = 220.0;

/// Branche l'onglet (compteurs et état de la vue du journal). Ajouté par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.init_resource::<LogViewState>();
    history::build(app);
}

/// Le diagnostic détaillé (passes de rendu, entités, sessions de mesure), replié sous les cartes.
pub(crate) struct Detail<'a> {
    pub snapshot: &'a DiagnosticsSnapshot,
    pub session: &'a mut MeasurementSession,
    pub panel: &'a mut PanelState,
    pub now_s: f64,
}

/// Tout ce que l'onglet dessine, séparé de Bevy pour être testé sans écran.
pub(crate) struct StatsView<'a> {
    pub state: &'a mut LogViewState,
    /// Le tampon du journal ; `None` : « Journal indisponible ».
    pub log: Option<&'a LogBuffer>,
    /// Nom de la session, repris dans l'en-tête des copies.
    pub session: &'a str,
    pub tiles: TileData<'a>,
    pub charts: ChartData<'a>,
    pub detail: Option<Detail<'a>>,
    /// Photographie du contexte pour « Copier le contexte complet », prise seulement au clic.
    pub context: &'a dyn Fn() -> ContextSnapshot,
}

#[cfg(test)]
impl<'a> StatsView<'a> {
    /// Une vue sans aucune donnée (tout s'affiche « — » ou « Pas de données »).
    pub(crate) fn empty(state: &'a mut LogViewState) -> Self {
        Self {
            state,
            log: None,
            session: logbuf::UNKNOWN_SESSION,
            tiles: TileData::default(),
            charts: ChartData::default(),
            detail: None,
            context: &ContextSnapshot::default,
        }
    }
}

/// Dessine l'onglet dans `ui` ; rend la hauteur de la zone des lignes du journal.
pub(crate) fn draw(ui: &mut egui::Ui, view: &mut StatsView<'_>) -> f32 {
    tiles::draw(ui, &view.tiles);
    let card_h = (ui.available_height() * CARD_SHARE)
        .min(CARD_MAX_H)
        .min(ui.available_height() - LOG_RESERVE_H);
    if card_h >= CARD_MIN_H {
        charts::draw(ui, &view.charts, card_h);
    }
    if let Some(detail) = view.detail.as_mut() {
        egui::CollapsingHeader::new("Diagnostic détaillé et sessions de mesure")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("stats_detail")
                    .max_height(DETAIL_MAX_H)
                    .show(ui, |ui| {
                        diagnostics::draw(ui, detail.snapshot, detail.session, detail.panel, detail.now_s);
                    });
            });
    }
    ui.separator();
    match view.log {
        Some(buffer) => {
            // Une seule copie du tampon par image : la barre d'outils, le filtre et les lignes la partagent.
            let live = buffer.snapshot();
            log_view::draw(ui, view.state, buffer, &live, view.session, view.context)
        }
        None => log_view::draw_unavailable(ui),
    }
}

/// Ce que l'onglet lit en plus de la coque.
#[derive(SystemParam)]
pub(crate) struct StatsParams<'w> {
    view: ResMut<'w, LogViewState>,
    log: Option<Res<'w, OperatorLog>>,
    recorder: Option<Res<'w, DebugLog>>,
    postfx: Option<Res<'w, PostFxState>>,
    tally: Res<'w, EventTally>,
    energy: Res<'w, EnergyHistory>,
    airtime: Res<'w, Airtime>,
    sheet: Option<Res<'w, ShowSheet>>,
    show: Option<Res<'w, CurrentShow>>,
    district: Option<Res<'w, DistrictState>>,
    mode: Res<'w, OperatorMode>,
    /// « Marquer ce moment » du bouton du journal : le même chemin que la touche `M`.
    marks: MarkParams<'w>,
}

/// L'onglet dans le panneau central de la fenêtre opérateur.
pub(crate) fn show(
    root: &mut egui::Ui,
    shell: &mut ShellParams,
    stats: &mut StatsParams,
    features: Option<&AudioFeatures>,
) {
    let now_s = shell.time.elapsed_secs_f64();
    let events_played = shell.active.is_some().then(|| stats.tally.total());
    let ShellParams {
        diagnostics,
        session,
        stats: panel,
        ..
    } = shell;
    let session_name = logbuf::session_name(stats.recorder.as_deref().map(|r| &r.0));
    let detail = match (diagnostics.as_deref(), session.as_deref_mut()) {
        (Some(snapshot), Some(session)) => Some(Detail {
            snapshot,
            session,
            panel,
            now_s,
        }),
        _ => None,
    };
    let sources = || {
        Sources {
            active: shell.active.as_deref(),
            clock: shell.clock.as_deref(),
            district: stats.district.as_deref(),
            show: stats.show.as_deref().map(|s| &s.0).zip(stats.sheet.as_deref()),
            automation: shell.automation.as_deref(),
            sim: shell.sim.as_deref(),
            mode: *stats.mode,
        }
        .snapshot()
    };
    let mut view = StatsView {
        state: &mut stats.view,
        log: stats.log.as_deref().map(|l| &l.0),
        session: &session_name,
        tiles: TileData {
            diagnostics: diagnostics.as_deref(),
            features,
            events_played,
            flashes_permitted: stats.postfx.as_deref().map(|p| p.show_fx().limiter().permitted()),
        },
        charts: ChartData {
            energy: Some(&stats.energy),
            features,
            tally: Some(&stats.tally),
            airtime: stats.sheet.as_deref().map(|sheet| (&*stats.airtime, sheet)),
        },
        detail,
        context: &sources,
    };
    egui::CentralPanel::default().show(root, |ui| {
        draw(ui, &mut view);
    });
    if stats.view.take_mark_request() && stats.marks.mark() {
        stats
            .view
            .notify("Repère posé dans le journal".to_owned(), now_s_ui(root));
    }
}

/// Le temps d'egui, celui que lit [`LogViewState::toast`].
fn now_s_ui(root: &egui::Ui) -> f64 {
    root.input(|i| i.time)
}
