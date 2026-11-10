//! Le branchement Bevy du moteur de routage : signaux filtrés, onde de kick et densité visuelle.

use bevy::prelude::*;
use cf_show::density::DensityState;

use super::engine::RoutingEngine;
use super::matrix::RoutingMatrix;
use super::signals::FilteredSignals;
use crate::CfSet;
use crate::audio::{AudioFeatures, Kick, VisualLatency};
use crate::show::CurrentShow;

/// La densité visuelle `Δ` (DA §4.2), publiée à côté de l'état du show : une copie de `ShowState::density` qui existe
/// aussi sans show chargé (valeur d'ouverture).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualDensity(pub DensityState);

/// Ajoute [`FilteredSignals`], [`RoutingEngine`] et [`VisualDensity`], et les met à jour en `CfSet::Show`, après le
/// chef du spectacle (la politique de flashs et `Δ` sont les siens).
#[derive(Debug, Default)]
pub struct RoutingPlugin;

impl Plugin for RoutingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FilteredSignals>()
            .init_resource::<VisualDensity>()
            .insert_resource(RoutingEngine::new(&RoutingMatrix::default_matrix()))
            .add_systems(
                Update,
                (route_signals, publish_density)
                    .chain()
                    .in_set(CfSet::Show)
                    .after(crate::show::run_show),
            );
    }
}

fn route_signals(
    mut engine: ResMut<RoutingEngine>,
    mut signals: ResMut<FilteredSignals>,
    mut kicks: MessageReader<Kick>,
    features: Res<AudioFeatures>,
    latency: Res<VisualLatency>,
    time: Res<Time<Real>>,
    show: Option<Res<CurrentShow>>,
) {
    let (flashes, breathing) = show.map_or((true, false), |s| (s.flashes, s.breathing));
    for _ in kicks.read() {
        signals.kick_wave.push();
    }
    engine.update(
        &mut signals,
        &features.0,
        time.delta_secs(),
        time.elapsed_secs_f64(),
        latency.seconds() as f32,
        (flashes, breathing),
    );
}

fn publish_density(show: Option<Res<CurrentShow>>, mut density: ResMut<VisualDensity>) {
    if let Some(show) = show {
        density.0 = show.0.density;
    }
}
