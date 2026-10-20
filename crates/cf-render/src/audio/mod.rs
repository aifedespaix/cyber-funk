//! Le pont audio (J1-06) : démarre les threads de `cf-audio` et `cf-analysis`, publie leurs résultats à Bevy en
//! `CfSet::Input`, applique le décalage visuel et reçoit les ordres de l'opérateur.
//!
//! [`CfAudioPlugin`] ne s'appelle pas `AudioPlugin` pour ne pas être confondu avec celui de Bevy, désactivé (pas de
//! feature `audio` : `bevy_audio` tirerait sa propre version de `cpal`).
//!
//! Ce que les autres crates lisent :
//! - `Res<AudioFeatures>` : l'état d'analyse de l'image, décalé du retard visuel ;
//! - messages [`Kick`] et [`SectionChanged`] : les impulsions, jamais perdues entre deux images ;
//! - `ResMut<VisualLatency>` : le décalage visuel, réglable à chaud ;
//! - `Res<AudioStatus>` et les messages [`AudioCommand`] : le panneau opérateur.

mod bridge;
mod config;
mod latency;
mod runtime;
mod types;

pub use bridge::AudioBridge;
pub use cf_analysis::{AnalysisStatsSnapshot, Section, TempoSource};
pub use cf_audio::capture::{CaptureState, DeviceSelector};
pub use cf_audio::player::PlayerState;
pub use cf_audio::playlist::{PlayMode, Track};
pub use config::{AudioConfig, InputBackendChoice, OutputChoice};
pub use latency::{LatencyLine, MAX_LATENCY_MS};
pub use runtime::AudioRuntime;
pub use types::{AudioCommand, AudioFeatures, AudioSource, AudioStatus, Kick, SectionChanged, VisualLatency};

use bevy::prelude::*;
use cf_analysis::AudioEventKind;

use crate::CfSet;
use runtime::STATUS_PERIOD_S;

/// Branche l'audio : threads, `Res<AudioFeatures>`, messages [`Kick`] et [`SectionChanged`], commandes opérateur.
///
/// À ajouter après `CorePlugin` (qui déclare `CfSet::Input`).
#[derive(Debug, Default)]
pub struct CfAudioPlugin {
    /// Réglages (backends, source de départ, décalage).
    pub config: AudioConfig,
}

impl CfAudioPlugin {
    /// Plugin avec ces réglages.
    #[must_use]
    pub fn new(config: AudioConfig) -> Self {
        Self { config }
    }
}

impl Plugin for CfAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioFeatures>()
            .init_resource::<AudioStatus>()
            .insert_resource(VisualLatency::new(self.config.latency_ms))
            .add_message::<Kick>()
            .add_message::<SectionChanged>()
            .add_message::<AudioCommand>();
        match AudioRuntime::start(&self.config) {
            Ok(mut runtime) => {
                runtime.refresh_devices();
                app.insert_non_send(runtime);
            }
            Err(error) => {
                tracing::error!(%error, "audio : pont désactivé");
                app.world_mut()
                    .resource_mut::<AudioStatus>()
                    .notes
                    .push(error.to_string());
            }
        }
        app.add_systems(
            Update,
            (apply_commands, pump_audio, refresh_status)
                .chain()
                .in_set(CfSet::Input),
        );
    }
}

/// Ordres de l'opérateur. Plusieurs changements de source dans la même image ne donnent qu'une bascule des threads.
fn apply_commands(runtime: Option<NonSendMut<AudioRuntime>>, mut commands: MessageReader<AudioCommand>) {
    let Some(mut runtime) = runtime else {
        commands.clear();
        return;
    };
    for command in commands.read() {
        runtime.apply(command);
    }
    // Aussi sans ordre : le changement de piste d'un lecteur qui enchaîne seul remet l'analyse à zéro.
    runtime.reconcile();
}

/// Lit l'analyse, applique le décalage visuel, publie la ressource et les messages.
fn pump_audio(
    runtime: Option<NonSendMut<AudioRuntime>>,
    time: Res<Time<Real>>,
    latency: Res<VisualLatency>,
    mut features: ResMut<AudioFeatures>,
    mut kicks: MessageWriter<Kick>,
    mut sections: MessageWriter<SectionChanged>,
) {
    let Some(mut runtime) = runtime else {
        return;
    };
    let mut kick = false;
    let mut section_changed = false;
    let shown = runtime
        .bridge_mut()
        .poll(time.elapsed_secs_f64(), latency.seconds(), |event| match event.kind {
            AudioEventKind::Kick => {
                kick = true;
                kicks.write(Kick {
                    audio_time_s: event.time_s,
                    step: event.step,
                });
            }
            AudioEventKind::SectionChanged { section } => {
                section_changed = true;
                sections.write(SectionChanged { section });
            }
            AudioEventKind::SourceChanged { .. } | AudioEventKind::LiveChanged { .. } => {}
        });
    if let Some(mut shown) = shown {
        // Les drapeaux d'un pas de 10,7 ms échappent à une image à 60 i/s : ils suivent les messages à la place.
        shown.kick = kick;
        shown.section_changed = section_changed;
        features.0 = shown;
    }
}

/// Rafraîchit l'instantané du panneau opérateur, une dizaine de fois par seconde.
fn refresh_status(runtime: Option<NonSendMut<AudioRuntime>>, time: Res<Time<Real>>, mut status: ResMut<AudioStatus>) {
    let Some(mut runtime) = runtime else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if now - runtime.last_status_s < STATUS_PERIOD_S {
        return;
    }
    runtime.last_status_s = now;
    runtime.fill_status(&mut status);
}

#[cfg(test)]
mod tests;
