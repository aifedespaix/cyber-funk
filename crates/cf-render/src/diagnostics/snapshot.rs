//! L'instantané des mesures de l'image, lu par le panneau opérateur et par la session CSV.
//!
//! Les sources sont celles de Bevy 0.20 (`DiagnosticsStore`) et celles de cyber-funk (`StreamStats`). Rien n'est
//! mesuré ici : ce module rassemble, au même endroit et au même instant, des chiffres venus de plusieurs systèmes.

use std::collections::HashSet;

use bevy::diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::diagnostic::MeshAllocatorDiagnosticPlugin;

use super::frame_times::FrameWindow;
use super::session::{MeasurementSession, SessionRow};
use crate::streaming::StreamStats;

/// Images gardées pour la moyenne et le p99 : 10 s à 60 i/s.
pub const FRAME_WINDOW: usize = 600;
/// Le comptage des maillages distincts parcourt toutes les entités visibles : on ne le refait que toutes les N images.
const MESH_KIND_EVERY: u32 = 10;

/// Échelle de rendu courante (1,0 = pleine résolution). Écrite par la qualité adaptative (J4-06) ; tant qu'elle n'existe
/// pas, la surcouche lit 1,0.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RenderScale(pub f32);

impl Default for RenderScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Temps d'une passe (ou d'un nœud) du graphe de rendu, tel que Bevy l'enregistre.
#[derive(Debug, Clone, PartialEq)]
pub struct PassTiming {
    /// Chemin de la passe, séparé par `/` quand elle est imbriquée dans une autre.
    pub name: String,
    /// Temps CPU d'enregistrement des commandes, en ms.
    pub cpu_ms: Option<f64>,
    /// Temps GPU mesuré par les requêtes de timestamp, en ms. `None` si le GPU ou le pilote n'en fournit pas.
    pub gpu_ms: Option<f64>,
}

impl PassTiming {
    /// Vrai pour une passe de premier niveau (non imbriquée) : seules celles-ci s'additionnent sans double compte.
    #[must_use]
    pub fn is_top_level(&self) -> bool {
        !self.name.contains('/')
    }
}

/// Les mesures de la dernière image.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct DiagnosticsSnapshot {
    /// Durée de la dernière image, en ms.
    pub frame_ms: f32,
    /// Moyenne des temps d'image sur la fenêtre, en ms.
    pub frame_mean_ms: f32,
    /// 99ᵉ percentile des temps d'image sur la fenêtre, en ms.
    pub frame_p99_ms: f32,
    /// Images i/s, déduites de la moyenne.
    pub fps: f32,
    /// Mesures dans la fenêtre (< `FRAME_WINDOW` juste après le démarrage).
    pub frame_samples: usize,
    /// Passes du graphe de rendu.
    pub passes: Vec<PassTiming>,
    /// Somme du temps GPU des passes de premier niveau. `None` : pas de timestamps GPU (rendu logiciel, Metal…).
    pub gpu_total_ms: Option<f64>,
    /// Entités du monde Bevy, tous types confondus.
    pub world_entities: u32,
    /// Entités qui peuvent être dessinées (portent une `ViewVisibility`).
    pub renderable_entities: u32,
    /// Parmi elles, celles que le culling garde visibles.
    pub visible_entities: u32,
    /// Maillages distincts parmi les entités visibles : borne basse du nombre de lots (donc de draw calls). Bevy 0.20
    /// n'expose pas de compteur de draw calls.
    pub visible_mesh_kinds: u32,
    /// Compteurs du streaming, si un monde streamé est actif.
    pub stream: Option<StreamStats>,
    /// Octets alloués pour les maillages sur le GPU (slabs de l'allocateur de maillages). Textures non comptées.
    pub gpu_mesh_bytes: Option<u64>,
    /// Échelle de rendu courante.
    pub render_scale: f32,
}

/// Fenêtre des temps d'image de la surcouche.
#[derive(Resource, Debug)]
pub(super) struct FrameTimes(pub FrameWindow);

impl Default for FrameTimes {
    fn default() -> Self {
        Self(FrameWindow::new(FRAME_WINDOW))
    }
}

/// Mémoire de travail du comptage des maillages distincts, conservée d'une image à l'autre.
#[derive(Default)]
pub(super) struct MeshKindScratch {
    seen: HashSet<AssetId<Mesh>>,
    frame: u32,
}

/// Met à jour l'instantané. Tourne dans `CfSet::Present`.
pub(super) fn collect_snapshot(
    time: Res<Time<Real>>,
    store: Res<DiagnosticsStore>,
    mut frames: ResMut<FrameTimes>,
    mut snapshot: ResMut<DiagnosticsSnapshot>,
    renderables: Query<(&ViewVisibility, Option<&Mesh3d>)>,
    stream: Option<Res<StreamStats>>,
    scale: Option<Res<RenderScale>>,
    mut scratch: Local<MeshKindScratch>,
) {
    let frame_ms = time.delta_secs() * 1000.0;
    // Une image sans durée (la toute première) n'est pas une mesure : elle ferait baisser la moyenne.
    if frame_ms > 0.0 {
        frames.0.push(frame_ms);
    }
    let snapshot = &mut *snapshot;
    snapshot.frame_ms = frame_ms;
    snapshot.frame_samples = frames.0.len();
    snapshot.frame_mean_ms = frames.0.mean().unwrap_or_default();
    snapshot.frame_p99_ms = frames.0.percentile(99.0).unwrap_or_default();
    snapshot.fps = if snapshot.frame_mean_ms > 0.0 {
        1000.0 / snapshot.frame_mean_ms
    } else {
        0.0
    };

    update_passes(&store, &mut snapshot.passes);
    snapshot.gpu_total_ms = snapshot
        .passes
        .iter()
        .filter(|p| p.is_top_level())
        .filter_map(|p| p.gpu_ms)
        .reduce(|a, b| a + b);

    snapshot.world_entities = store
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.value())
        .map_or(0, |v| v as u32);
    snapshot.gpu_mesh_bytes = store
        .get(MeshAllocatorDiagnosticPlugin::slabs_size_diagnostic_path())
        .and_then(|d| d.value())
        .map(|v| v as u64);

    let mut total = 0;
    let mut visible = 0;
    let count_kinds = scratch.frame % MESH_KIND_EVERY == 0;
    scratch.frame = scratch.frame.wrapping_add(1);
    if count_kinds {
        scratch.seen.clear();
    }
    for (view, mesh) in &renderables {
        total += 1;
        if view.get() {
            visible += 1;
            if let (true, Some(mesh)) = (count_kinds, mesh) {
                scratch.seen.insert(mesh.0.id());
            }
        }
    }
    snapshot.renderable_entities = total;
    snapshot.visible_entities = visible;
    if count_kinds {
        snapshot.visible_mesh_kinds = scratch.seen.len() as u32;
    }

    snapshot.stream = stream.map(|s| *s);
    snapshot.render_scale = scale.map_or(1.0, |s| s.0);
}

/// Range les diagnostics `render/<passe>/elapsed_{cpu,gpu}` de Bevy dans la liste des passes, en place : les noms ne
/// sont alloués qu'à la première apparition d'une passe.
fn update_passes(store: &DiagnosticsStore, passes: &mut Vec<PassTiming>) {
    for diagnostic in store.iter() {
        let path = diagnostic.path().as_str();
        let Some(rest) = path.strip_prefix("render/") else {
            continue;
        };
        let (name, is_gpu) = if let Some(name) = rest.strip_suffix("/elapsed_gpu") {
            (name, true)
        } else if let Some(name) = rest.strip_suffix("/elapsed_cpu") {
            (name, false)
        } else {
            continue;
        };
        let index = if let Some(index) = passes.iter().position(|p| p.name == name) {
            index
        } else {
            passes.push(PassTiming {
                name: name.to_owned(),
                cpu_ms: None,
                gpu_ms: None,
            });
            passes.len() - 1
        };
        if let Some(pass) = passes.get_mut(index) {
            let value = diagnostic.value();
            if is_gpu {
                pass.gpu_ms = value;
            } else {
                pass.cpu_ms = value;
            }
        }
    }
}

/// Ajoute l'image courante à la session si elle enregistre. Tourne après [`collect_snapshot`].
pub(super) fn record_session(
    time: Res<Time<Real>>,
    snapshot: Res<DiagnosticsSnapshot>,
    mut session: ResMut<MeasurementSession>,
) {
    if !session.is_recording() {
        return;
    }
    let stream = snapshot.stream.unwrap_or_default();
    session.record(
        time.elapsed_secs_f64(),
        SessionRow {
            t_s: 0.0,
            frame_ms: snapshot.frame_ms,
            gpu_ms: snapshot.gpu_total_ms,
            world_entities: snapshot.world_entities,
            renderable_entities: snapshot.renderable_entities,
            visible_entities: snapshot.visible_entities,
            visible_mesh_kinds: snapshot.visible_mesh_kinds,
            chunks_live: stream.chunks_live,
            pieces_used: stream.pieces_used,
            render_scale: snapshot.render_scale,
        },
    );
}
