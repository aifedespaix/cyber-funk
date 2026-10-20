//! Surcouche de diagnostic (J4-05) : les mesures de l'image rassemblées en un instantané pour le panneau de la
//! fenêtre opérateur, et une session de mesure exportable en CSV.
//!
//! Jamais dessinée sur le projecteur : ce crate ne produit que des données, le panneau vit dans `cf-operator`.
//!
//! Sources, dans Bevy 0.20 :
//! - temps d'image : `Time<Real>`, dans une fenêtre glissante (moyenne, p99) ;
//! - temps CPU et GPU par passe : `RenderDiagnosticsPlugin` (diagnostics `render/<passe>/elapsed_{cpu,gpu}`).
//!   Le temps GPU exige `TIMESTAMP_QUERY` (Vulkan, DX12) ; sinon il reste absent et seul le CPU est publié ;
//! - entités : `EntityCountDiagnosticsPlugin`, et `ViewVisibility` pour visibles / dessinables ;
//! - mémoire GPU : slabs de l'allocateur de maillages (maillages seulement ; Bevy n'expose pas la mémoire des textures) ;
//! - tronçons : `StreamStats` (J4-03) ;
//! - draw calls : Bevy 0.20 n'expose aucun compteur ; on publie le nombre de maillages distincts visibles, borne
//!   basse du nombre de lots.

mod frame_times;
mod session;
mod snapshot;

use bevy::diagnostic::EntityCountDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::diagnostic::{MeshAllocatorDiagnosticPlugin, RenderDiagnosticsPlugin};

pub use frame_times::FrameWindow;
pub use session::{CSV_HEADER, ExportError, MeasurementSession, SessionRow};
pub use snapshot::{DiagnosticsSnapshot, FRAME_WINDOW, PassTiming, RenderScale};

use crate::CfSet;
use snapshot::{FrameTimes, collect_snapshot, record_session};

/// Branche la surcouche de diagnostic : instantané des mesures et session de mesure.
///
/// À ajouter après `DefaultPlugins` (les diagnostics de rendu s'accrochent au sous-monde de rendu, absent sous
/// `MinimalPlugins` : le plugin le détecte et se limite alors aux mesures du monde principal).
#[derive(Debug)]
pub struct DiagnosticsPlugin {
    /// Active les mesures de rendu par passe (timestamps GPU quand le matériel les fournit). Elles ajoutent des
    /// requêtes GPU et une lecture en retour par image : à couper pour mesurer le coût de la surcouche elle-même.
    pub render_passes: bool,
}

impl Default for DiagnosticsPlugin {
    fn default() -> Self {
        Self { render_passes: true }
    }
}

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() {
            app.add_plugins(EntityCountDiagnosticsPlugin::default());
        }
        if self.render_passes && app.get_sub_app(RenderApp).is_some() {
            app.add_plugins((RenderDiagnosticsPlugin, MeshAllocatorDiagnosticPlugin));
        }
        app.init_resource::<DiagnosticsSnapshot>()
            .init_resource::<RenderScale>()
            .init_resource::<FrameTimes>()
            .init_resource::<MeasurementSession>()
            .add_systems(Update, (collect_snapshot, record_session).chain().in_set(CfSet::Present));
    }
}

#[cfg(test)]
mod tests;
