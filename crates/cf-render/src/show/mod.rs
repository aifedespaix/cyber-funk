//! Le pont du chef du spectacle : appelle `ShowDirector::update` une fois par image en `CfSet::Show` et publie le
//! `ShowState` de l'image (`Res<CurrentShow>`) ainsi que les faits du spectacle (messages [`ShowEventMessage`]).
//!
//! ```text
//! CfSet::Input   ControlAction (clavier, MIDI, OSC)   Res<AudioFeatures> (tempo arbitré compris)
//!                         └──────────────┬───────────────────┘
//! CfSet::Show            run_show : ShowDirector::update ─▶ Res<CurrentShow>, messages ShowEventMessage
//!                        apply_slot_profiles : SlotLoaded ─▶ Mapper::reload (relâche les boutons tenus),
//!                                              source de tempo préférée du créneau
//! ```

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use cf_control::Action;
use cf_show::director::{ShowDirector, ShowEvent, ShowInput, ShowState};
use cf_show::drop::DropTitles;
use cf_show::load::LoadedShow;
use cf_show::model::ControlProfileId;
use rand_pcg::Pcg32;

use crate::CfSet;
use crate::audio::AudioFeatures;
use crate::controls::ControlAction;

/// Secondes dans un jour.
const DAY_S: u64 = 86_400;

/// L'état du spectacle de l'image, écrit par le seul `run_show`. Absent tant qu'aucun show n'est chargé.
///
/// Newtype : `ShowState` est défini dans `cf-show`, qui ne dépend pas de Bevy et ne peut pas en dériver `Resource`
/// (même principe que `AudioFeatures`).
#[derive(Resource, Debug, Clone, Deref)]
pub struct CurrentShow(pub ShowState);

/// Un fait du spectacle (cue entré, créneau chargé, titre…), publié une fois, à lire en `CfSet::Simulate` ou après.
#[derive(Message, Debug, Clone, PartialEq, Deref)]
pub struct ShowEventMessage(pub ShowEvent);

/// Les profils de contrôle et les préférences de tempo de l'événement, extraits une fois à l'ouverture.
#[derive(Resource, Debug, Clone, Default)]
pub struct ShowControls {
    /// Profils communs à la soirée.
    pub event: Vec<ControlProfileId>,
    /// Profils propres à chaque créneau, dans l'ordre des créneaux.
    pub slots: Vec<Vec<ControlProfileId>>,
    /// Source de tempo préférée de chaque créneau (`None` : arbitrage automatique).
    pub tempo: Vec<Option<cf_analysis::TempoSource>>,
}

impl ShowControls {
    /// Extrait ce qu'il faut d'un événement chargé.
    #[must_use]
    pub fn from_show(show: &LoadedShow) -> Self {
        use cf_show::model::TempoPreference;
        let event = show.event();
        Self {
            event: event.settings.controls.clone(),
            slots: event.slots.iter().map(|slot| slot.controls.clone()).collect(),
            tempo: event
                .slots
                .iter()
                .map(|slot| match slot.audio.tempo_source {
                    TempoPreference::Auto => None,
                    TempoPreference::Prefer(source) => Some(source),
                })
                .collect(),
        }
    }
}

/// Le chef du spectacle et ses tampons.
#[derive(Resource)]
pub struct ShowRuntime {
    director: ShowDirector<Pcg32>,
    /// Actions de l'image, réutilisées d'une image à l'autre (aucune allocation en régime établi).
    actions: Vec<Action>,
    utc_offset_s: f64,
}

impl fmt::Debug for ShowRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShowRuntime")
            .field("director", &self.director)
            .finish_non_exhaustive()
    }
}

impl ShowRuntime {
    /// Le chef du spectacle, pour les systèmes qui prennent un ordre (`take`, `take_trigger`) ou marquent la fraîcheur.
    pub fn director_mut(&mut self) -> &mut ShowDirector<Pcg32> {
        &mut self.director
    }
}

/// Branche le chef du spectacle d'un événement chargé.
///
/// À ajouter après `CorePlugin` et, pour que les actions et le tempo soient à jour, `ControlsPlugin` et
/// `CfAudioPlugin`. Sans show (`show: None`), le plugin ne fait rien : pas de `CurrentShow`, les contrôles tournent
/// quand même.
#[derive(Debug)]
pub struct ShowPlugin {
    /// L'événement validé, ou rien.
    pub show: Option<LoadedShow>,
    /// Cartons-titres des drops.
    pub titles: DropTitles,
    /// Graine de tout l'aléatoire du spectacle.
    pub seed: u64,
    /// Décalage de l'heure locale sur UTC, en secondes (la bibliothèque standard ne donne pas le fuseau). Sert aux
    /// avancées « à 01:00 » des cues.
    pub utc_offset_s: f64,
}

impl ShowPlugin {
    /// Plugin sur un show chargé, sans titres, heure locale = UTC.
    #[must_use]
    pub fn new(show: LoadedShow) -> Self {
        Self {
            show: Some(show),
            titles: DropTitles::empty(),
            seed: 0,
            utc_offset_s: 0.0,
        }
    }
}

impl Plugin for ShowPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShowEventMessage>();
        let Some(show) = &self.show else {
            return;
        };
        match ShowDirector::<Pcg32>::new(show, self.titles.clone(), self.seed) {
            Ok(director) => {
                app.insert_resource(CurrentShow(director.state().clone()))
                    .insert_resource(ShowControls::from_show(show))
                    .insert_resource(ShowRuntime {
                        director,
                        actions: Vec::new(),
                        utc_offset_s: self.utc_offset_s,
                    });
                app.add_systems(
                    Update,
                    // L'horloge du show ne court pas pendant le chargement (J4-07).
                    (
                        run_show.run_if(crate::warmup::show_may_run),
                        crate::controls::apply_slot_profiles,
                    )
                        .chain()
                        .in_set(CfSet::Show),
                );
            }
            Err(error) => tracing::error!(%error, "spectacle : chef du spectacle indisponible"),
        }
    }
}

/// Heure locale en secondes depuis minuit.
fn local_time_s(utc_offset_s: f64) -> f64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    (since_epoch + utc_offset_s).rem_euclid(DAY_S as f64)
}

/// Une image du spectacle : actions reçues + analyse audio ─▶ `ShowState` et faits.
fn run_show(
    mut runtime: ResMut<ShowRuntime>,
    mut current: ResMut<CurrentShow>,
    time: Res<Time<Real>>,
    features: Res<AudioFeatures>,
    mut received: MessageReader<ControlAction>,
    mut events: MessageWriter<ShowEventMessage>,
) {
    let runtime = &mut *runtime;
    runtime.actions.clear();
    runtime.actions.extend(received.read().map(|a| a.0.action.clone()));
    let input = ShowInput {
        dt: time.delta_secs(),
        now: time.elapsed_secs_f64(),
        local_time_s: local_time_s(runtime.utc_offset_s),
        audio: &features.0,
        // Aucun détecteur de drop réel dans l'analyse pour l'instant : le chef d'orchestre du drop se fie à
        // l'intensité (voir `DropConductor`) et à l'action `ForceDrop`.
        real_drop: false,
        actions: &runtime.actions,
    };
    current.0.clone_from(runtime.director.update(&input));
    for event in runtime.director.drain_events() {
        events.write(ShowEventMessage(event));
    }
}

#[cfg(test)]
mod tests;
