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

mod sheet;

pub use sheet::{SheetCue, SheetSlot, ShowSheet};

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use cf_control::Action;
use cf_show::director::{ShowDirector, ShowEvent, ShowInput, ShowState};
use cf_show::drop::DropTitles;
use cf_show::guard::{Guard, GuardOutcome};
use cf_show::load::LoadedShow;
use cf_show::model::ControlProfileId;
use rand_pcg::Pcg32;

use crate::CfSet;
use crate::audio::AudioFeatures;
use crate::controls::ControlAction;
use crate::postfx::{CurrentShowFx, ShowFxInput};
use crate::sim::{SimClock, SimReset};

/// Secondes dans un jour.
const DAY_S: u64 = 86_400;

/// L'état du spectacle de l'image, écrit par le seul `run_show`. Absent tant qu'aucun show n'est chargé.
///
/// Le DJ par lequel repartira le prochain `Démarrer`, désigné par l'opérateur à l'arrêt. `None` : le DJ en cours (celui
/// que le chef du spectacle avait au moment de l'arrêt). Consommé par le reset.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartSlot(pub Option<usize>);

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
    /// Isole une panique du chef du spectacle : le show garde le dernier état publié (skill `live-safety`).
    guard: Guard,
    /// De quoi reconstruire le chef du spectacle au reset de l'opérateur.
    source: ShowSource,
}

/// L'événement chargé, ses titres et sa graine : tout ce qu'il faut pour recréer un chef du spectacle neuf.
#[derive(Debug, Clone)]
struct ShowSource {
    show: LoadedShow,
    titles: DropTitles,
    seed: u64,
}

impl fmt::Debug for ShowRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShowRuntime")
            .field("director", &self.director)
            .finish_non_exhaustive()
    }
}

impl ShowRuntime {
    /// Le chef du spectacle en lecture seule : drop préparé, créneaux (interface opérateur).
    #[must_use]
    pub fn director(&self) -> &ShowDirector<Pcg32> {
        &self.director
    }

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
                // Avant la première image du show (chargement), aucun flash ni vague : état sûr (règle 7).
                let quiet = ShowFxInput {
                    flashes: false,
                    ..ShowFxInput::from_show(director.state(), None)
                };
                app.insert_resource(CurrentShowFx(quiet))
                    .insert_resource(CurrentShow(director.state().clone()))
                    .insert_resource(ShowControls::from_show(show))
                    .insert_resource(ShowSheet::from_show(show))
                    .insert_resource(ShowRuntime {
                        director,
                        actions: Vec::new(),
                        utc_offset_s: self.utc_offset_s,
                        // 3 images de suite en panique : le chef est désactivé, le show garde son dernier état.
                        guard: Guard::new(3),
                        source: ShowSource {
                            show: show.clone(),
                            titles: self.titles.clone(),
                            seed: self.seed,
                        },
                    });
                app.add_plugins(crate::run::RunPlugin);
                app.init_resource::<StartSlot>()
                    .add_systems(Update, reset_show.in_set(CfSet::Input).after(crate::sim::apply_control));
                app.add_systems(
                    Update,
                    // L'horloge du show ne court pas pendant le chargement (J4-07).
                    (
                        (run_show, publish_show_fx)
                            .chain()
                            .run_if(crate::warmup::show_may_run)
                            .run_if(crate::run::show_armed),
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
pub(crate) fn run_show(
    mut runtime: ResMut<ShowRuntime>,
    mut current: ResMut<CurrentShow>,
    time: Res<Time<Real>>,
    clock: Res<SimClock>,
    features: Res<AudioFeatures>,
    mut received: MessageReader<ControlAction>,
    mut events: MessageWriter<ShowEventMessage>,
) {
    let runtime = &mut *runtime;
    runtime.actions.clear();
    runtime.actions.extend(received.read().map(|a| a.0.action.clone()));
    let input = ShowInput {
        dt: time.delta_secs(),
        // Horloge de simulation : elle ne compte pas les pauses, le chef ne voit donc pas de saut à la reprise.
        now: clock.sim_s(),
        local_time_s: local_time_s(runtime.utc_offset_s),
        audio: &features.0,
        // Aucun détecteur de drop réel dans l'analyse pour l'instant : le chef d'orchestre du drop se fie à
        // l'intensité (voir `DropConductor`) et à l'action `ForceDrop`.
        real_drop: false,
        actions: &runtime.actions,
    };
    let ShowRuntime { director, guard, .. } = runtime;
    let outcome = guard.run(|| current.0.clone_from(director.update(&input)));
    match outcome {
        GuardOutcome::Ran => {}
        GuardOutcome::Panicked { streak } => tracing::error!(streak, "panique du chef du spectacle, image ignorée"),
        GuardOutcome::Disabled => {
            tracing::error!("le chef du spectacle a paniqué trop souvent : désactivé, le dernier état reste affiché");
        }
        GuardOutcome::Skipped => return,
    }
    for event in director.drain_events() {
        events.write(ShowEventMessage(event));
    }
}

/// Reset de l'opérateur : un chef du spectacle neuf (retour au premier cue du premier créneau), et le garde-fou de
/// panique réarmé. C'est ce qui sort le show d'un état « désactivé après trois paniques » où l'image reste figée
/// pendant que la musique continue.
fn reset_show(
    mut resets: MessageReader<SimReset>,
    runtime: Option<ResMut<ShowRuntime>>,
    current: Option<ResMut<CurrentShow>>,
    start: Option<ResMut<StartSlot>>,
) {
    if resets.read().count() == 0 {
        return;
    }
    let (Some(mut runtime), Some(mut current)) = (runtime, current) else {
        return;
    };
    let source = runtime.source.clone();
    // Le DJ désigné par l'opérateur, sinon celui qui jouait : un arrêt puis un démarrage ne ramènent pas au premier.
    let slot = start.and_then(|mut s| s.0.take()).unwrap_or(current.0.slot);
    match ShowDirector::<Pcg32>::new_at(&source.show, source.titles, source.seed, slot) {
        Ok(director) => {
            current.0.clone_from(director.state());
            runtime.director = director;
            runtime.guard.reset();
            runtime.actions.clear();
        }
        Err(error) => tracing::error!(%error, "reset du spectacle : chef du spectacle indisponible"),
    }
}

/// Recopie de l'état du show ce que lisent les effets (post-process, voiture, ciel) : flashs permis, respiration, chaleur,
/// drop qui frappe. Seul écrivain de [`CurrentShowFx`] ; chaque flash demandé passe ensuite par le `FlashLimiter` du
/// post-process, qui plafonne la sortie finale (règle 7).
fn publish_show_fx(runtime: Res<ShowRuntime>, current: Res<CurrentShow>, mut fx: ResMut<CurrentShowFx>) {
    let next = ShowFxInput::from_show(&current.0, runtime.director.drop_plan());
    // Écriture conditionnelle : pas de détection de changement déclenchée à vide.
    if fx.0 != next {
        fx.0 = next;
    }
}

#[cfg(test)]
mod tests;
