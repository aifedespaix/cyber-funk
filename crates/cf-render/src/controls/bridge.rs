//! L'état du pont des contrôles et ses systèmes de lecture : MIDI, tempo.
//!
//! ```text
//! clavier (keyboard.rs) ─┐
//! thread MIDI ───────────┼─▶ Mapper ─▶ TimedAction ─▶ message ControlAction ─▶ chef du spectacle (CfSet::Show)
//!                        │                  └─ TapTempo / Resync / NudgePhase : routés ici, vers le tap et l'arbitre
//! horloge MIDI ──────────┴─▶ MidiClock ─┐
//! AudioFeatures ─▶ AudioBeatGrid ───────┼─▶ TempoArbiter ─▶ publish(&mut AudioFeatures)
//! TapTempo ─────────────────────────────┘
//! ```
//!
//! Le tempo est arbitré une fois par image, côté application et non dans le thread d'analyse : c'est ici que se
//! rencontrent l'audio (déjà décalé de la latence visuelle) et les contrôles.

use std::time::Instant;

use bevy::prelude::*;
use cf_analysis::{AudioBeatGrid, TempoArbiter, TempoReading, TempoSource};
use cf_control::{
    Action, ControlProfile, Dispatch, Learner, Mapper, MidiClock, MidiInput, MidiItem, PortEvent, RawInput, TapTempo,
    TempoEstimate, TimedAction, dispatch,
};

use super::ControlAction;
use super::status::{ControlsStatus, LastInput};
use crate::audio::AudioFeatures;

/// Messages MIDI lus au plus par image : un canal plein ne doit pas retenir l'image (le reste est lu à la suivante).
const MAX_MIDI_PER_FRAME: usize = 4096;
/// Période de rafraîchissement des ports et des compteurs du panneau (s).
const STATUS_PERIOD_S: f64 = 0.25;

/// Tout ce que le pont garde entre deux images. Une seule ressource : les systèmes d'entrée se suivent de toute façon.
#[derive(Resource, Debug)]
pub struct ControlBridge {
    pub(super) mapper: Mapper,
    pub(super) learner: Learner,
    pub(super) midi: Option<MidiInput>,
    pub(super) clock: MidiClock,
    pub(super) tap: TapTempo,
    pub(super) arbiter: TempoArbiter,
    pub(super) grid: AudioBeatGrid,
    /// Base profil du bas de la pile : le clavier par défaut de Sillage.
    pub(super) keyboard: ControlProfile,
    pub(super) library: super::ControllerLibrary,
    /// Origine de l'horloge des contrôles ; la même que l'`epoch` du MIDI, pour que tous les `time_s` s'accordent.
    epoch: Instant,
    last_status_s: f64,
    /// Tampon des relâchements, réutilisé.
    released: Vec<TimedAction>,
}

impl ControlBridge {
    pub(super) fn new(
        midi: Option<MidiInput>,
        library: super::ControllerLibrary,
        epoch: Instant,
    ) -> Result<Self, cf_control::Error> {
        let keyboard = ControlProfile::sillage_keyboard();
        let (mapper, _overrides) = Mapper::from_profiles(&[&keyboard])?;
        Ok(Self {
            mapper,
            learner: Learner::new(),
            midi,
            clock: MidiClock::new(),
            tap: TapTempo::new(),
            arbiter: TempoArbiter::new(),
            grid: AudioBeatGrid::new(),
            keyboard,
            library,
            epoch,
            last_status_s: f64::NEG_INFINITY,
            released: Vec::new(),
        })
    }

    /// Secondes depuis le démarrage des contrôles : l'horloge monotone commune (MIDI, clavier, tempo).
    #[must_use]
    pub fn now_s(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// Le tempo arbitré, pour l'interface et les tests.
    #[must_use]
    pub fn arbiter(&self) -> &TempoArbiter {
        &self.arbiter
    }

    /// Le mapper en place (table effective, noms de ports).
    #[must_use]
    pub fn mapper(&self) -> &Mapper {
        &self.mapper
    }
}

/// Publie les actions : message Bevy, dernière action du panneau, routage des actions de tempo.
pub(super) struct Emitter<'a, 'w> {
    pub bridge: &'a mut ControlBridge,
    pub status: &'a mut ControlsStatus,
    pub out: &'a mut MessageWriter<'w, ControlAction>,
}

impl Emitter<'_, '_> {
    /// Passe une entrée brute dans le mapper et publie l'action qui en sort.
    pub fn map(&mut self, raw: &RawInput) {
        if let Some(timed) = self.bridge.mapper.map(raw) {
            self.emit(timed);
        }
    }

    /// Relâche tout bouton à maintien encore tenu et publie les relâchements.
    pub fn release_held(&mut self, now_s: f64) {
        let mut buffer = std::mem::take(&mut self.bridge.released);
        self.bridge.mapper.release_held(now_s, &mut buffer);
        for timed in buffer.drain(..) {
            self.emit(timed);
        }
        self.bridge.released = buffer;
    }

    /// Publie une action. Le spectacle ignore les actions de tempo, et le tempo ignore les autres : on route ici.
    pub fn emit(&mut self, timed: TimedAction) {
        match &timed.action {
            Action::TapTempo => self.bridge.tap.tap(timed.time_s),
            Action::Resync => self.bridge.arbiter.resync(timed.time_s),
            Action::NudgePhase { beats } => self.bridge.arbiter.nudge(f64::from(*beats)),
            _ => {}
        }
        self.status.last_action = Some((timed.time_s, timed.action.clone()));
        self.out.write(ControlAction(timed));
    }
}

/// Vide le flux MIDI : actions, horloge, ports ; tient aussi le panneau à jour.
pub(super) fn read_midi(
    mut bridge: ResMut<ControlBridge>,
    mut status: ResMut<ControlsStatus>,
    mut out: MessageWriter<ControlAction>,
) {
    let now = bridge.now_s();
    let mut emitter = Emitter {
        bridge: &mut bridge,
        status: &mut status,
        out: &mut out,
    };
    for _ in 0..MAX_MIDI_PER_FRAME {
        let Some(item) = emitter.bridge.midi.as_ref().and_then(MidiInput::try_recv) else {
            break;
        };
        let event = match &item {
            MidiItem::Message(event) => Some(*event),
            MidiItem::Port(_) => None,
        };
        let result = {
            let bridge = &mut *emitter.bridge;
            dispatch(item, &mut bridge.mapper, &mut bridge.learner)
        };
        match result {
            Dispatch::Action(timed) => {
                emitter.status.last_input = event.map(LastInput::Midi);
                emitter.emit(timed);
            }
            Dispatch::Learned(binding) => {
                // Le panneau d'apprentissage arrive avec l'interface de préparation ; en attendant, la trace suffit.
                tracing::info!(source = ?binding.source, "MIDI learn : geste reconnu");
            }
            Dispatch::Port(PortEvent::Disconnected { port, .. }) => {
                // L'horloge d'un port qui disparaît ne doit pas rester en roue libre sur de vieilles impulsions.
                if emitter.bridge.clock.port() == Some(port) {
                    emitter.bridge.clock.reset();
                }
            }
            Dispatch::Port(PortEvent::Connected { .. }) => {}
            Dispatch::Other(event) => {
                // Les 24 impulsions par noire rempliraient le « dernier message » : seul le reste s'affiche.
                if !emitter.bridge.clock.handle(&event) {
                    emitter.status.last_input = Some(LastInput::Midi(event));
                }
            }
            Dispatch::Ignored => {
                if let Some(event) = event {
                    emitter.status.last_input = Some(LastInput::Midi(event));
                }
            }
        }
    }
    refresh_status(&mut bridge, &mut status, now);
}

/// Ports et compteurs : quelques fois par seconde seulement (l'instantané des ports prend un verrou et clone).
fn refresh_status(bridge: &mut ControlBridge, status: &mut ControlsStatus, now_s: f64) {
    status.clock_bpm = bridge.clock.bpm();
    status.transport = bridge.clock.transport();
    status.tap_bpm = bridge.tap.bpm();
    if now_s - bridge.last_status_s < STATUS_PERIOD_S {
        return;
    }
    bridge.last_status_s = now_s;
    if let Some(midi) = &bridge.midi {
        status.midi_running = true;
        status.ports = midi.ports();
        status.midi_delivered = midi.stats().delivered();
        status.midi_dropped = midi.stats().dropped();
    }
}

/// Copie champ pour champ l'estimation de `cf-control` (qui ne dépend pas de `cf-analysis`) en lecture de l'arbitre.
fn reading(estimate: TempoEstimate) -> TempoReading {
    TempoReading {
        time_s: estimate.time_s,
        beat: estimate.beat,
        bpm: estimate.bpm,
        bar_aligned: estimate.bar_aligned,
        confidence: estimate.confidence,
    }
}

/// Arbitre le tempo de l'image et le publie dans `AudioFeatures`. Après `AudioPump` : c'est l'état fraîchement lu qu'on
/// complète.
pub(super) fn update_tempo(mut bridge: ResMut<ControlBridge>, mut features: ResMut<AudioFeatures>) {
    let now = bridge.now_s();
    let bridge = &mut *bridge;
    // `AudioPump` ne réécrit la ressource que quand l'historique est assez long. Sinon, `features` porte encore notre
    // propre tempo arbitré de l'image d'avant : le redonner à la grille ferait boucler l'arbitre sur lui-même.
    // (Notre propre écriture ne compte pas comme un changement pour ce système.)
    if features.is_changed() {
        let f = &features.0;
        let audio = bridge.grid.observe(now, f.live, f.bpm, f.kick);
        bridge.arbiter.offer(TempoSource::Audio, audio);
    }
    bridge
        .arbiter
        .offer(TempoSource::Clock, bridge.clock.estimate().map(reading));
    bridge
        .arbiter
        .offer(TempoSource::Tap, bridge.tap.estimate().map(reading));
    bridge.arbiter.update(now);
    bridge.arbiter.publish(&mut features.0);
}
