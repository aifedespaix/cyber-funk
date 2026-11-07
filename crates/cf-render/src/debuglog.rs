//! Le pont entre Bevy et le journal de session (`cf-debuglog`) : tout ce que le spectacle fait d'important est écrit une
//! fois, avec l'horloge de simulation, pour qu'un plantage ou une incohérence se relise après coup.
//!
//! Ce qui est journalisé :
//!
//! | `ev`                                         | Quand                                               |
//! | -------------------------------------------- | --------------------------------------------------- |
//! | `cue.active` (start/end)                     | un cue est à l'écran (début à l'entrée, fin à la sortie) |
//! | `cue.called`, `slot.loaded`, `slot.preload`, `handover`, `dj.show`, `dj.logo`, `title`, `toggle` | faits du chef du spectacle |
//! | `action`, `world.trigger`                    | ordre de l'opérateur ou du MIDI (hors potards continus) |
//! | `sim.pause`, `sim.resume`, `sim.reset`       | commandes de l'opérateur                            |
//! | `perf.slow_frame`                            | image de plus de 100 ms (carte graphique saturée, gel) |
//! | `audio.track`, `audio.underrun`, `audio.device_error` | lecteur de démo                              |
//! | `watchdog.*`, `panic`, `log`                 | gel, panique, avertissements et erreurs `tracing`   |
//!
//! Aucun système chaud n'écrit : les faits sont rares, le battement de cœur part une fois par seconde, et l'écriture
//! disque se fait sur le thread de `cf-debuglog`.

use bevy::prelude::*;
use cf_control::ActionFamily;
use cf_debuglog::{Level, Recorder, Span};
use cf_show::director::ShowEvent;
use serde_json::{Value, json};

use crate::CfSet;
use crate::audio::AudioStatus;
use crate::controls::ControlAction;
use crate::diagnostics::DiagnosticsSnapshot;
use crate::show::{CurrentShow, ShowEventMessage, ShowSheet};
use crate::sim::{SimClock, SimControl, update_clock};

/// Image plus longue que ceci (ms) : signalée, car c'est le signe d'une carte graphique ou d'un disque saturés.
const SLOW_FRAME_MS: f32 = 100.0;
/// Pas minimal (s) entre deux signalements d'images lentes : un gel de plusieurs secondes ne fait pas mille lignes.
const SLOW_FRAME_REPEAT_S: f64 = 0.5;
/// Période (s) du battement de cœur.
const PULSE_PERIOD_S: f64 = 1.0;

/// Accès au journal de session depuis les systèmes Bevy.
#[derive(Resource, Debug, Clone)]
pub struct DebugLog(pub Recorder);

/// Branche le journal de session sur le spectacle. N'est ajouté que si l'enregistreur a pu démarrer.
#[derive(Debug, Clone)]
pub struct DebugLogPlugin {
    /// L'enregistreur ouvert par le binaire.
    pub recorder: Recorder,
}

impl Plugin for DebugLogPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugLog(self.recorder.clone()))
            // Idempotents : les plugins du spectacle et des contrôles enregistrent les mêmes messages.
            .add_message::<ShowEventMessage>()
            .add_message::<ControlAction>()
            .add_systems(Update, pulse.in_set(CfSet::Input).after(update_clock))
            .add_systems(Update, log_actions.in_set(CfSet::Show))
            .add_systems(Update, log_show_events.in_set(CfSet::Simulate));
    }
}

/// Référence stable `slot<N>/cue<M>` et libellé lisible « DJ · cue » d'un cue.
fn cue_names(sheet: Option<&ShowSheet>, slot: usize, cue: usize) -> (String, String) {
    let reference = format!("slot{slot}/cue{cue}");
    let label = sheet
        .and_then(|sheet| {
            let s = sheet.slots.get(slot)?;
            Some(format!("{} · {}", s.dj, s.cues.get(cue)?.name))
        })
        .unwrap_or_default();
    (reference, label)
}

/// Faits du chef du spectacle ─▶ journal.
fn log_show_events(
    log: Res<DebugLog>,
    sheet: Option<Res<ShowSheet>>,
    mut events: MessageReader<ShowEventMessage>,
    mut active: Local<Option<Span>>,
) {
    let sheet = sheet.as_deref();
    for ShowEventMessage(event) in events.read() {
        let rec = &log.0;
        match event {
            ShowEvent::CueEntered {
                slot,
                cue,
                previous_slot,
                previous_cue,
                cause,
            } => {
                if let Some(previous) = active.take() {
                    rec.end(&previous, json!({}));
                }
                let (reference, label) = cue_names(sheet, *slot, *cue);
                *active = Some(rec.begin(
                    "cue.active",
                    Some(&reference),
                    Some(&label),
                    json!({ "cause": format!("{cause:?}"), "from": format!("slot{previous_slot}/cue{previous_cue}") }),
                ));
            }
            ShowEvent::CueCalled {
                slot,
                cue,
                entry,
                cause,
            } => {
                let (reference, label) = cue_names(sheet, *slot, *cue);
                rec.point(
                    Level::Info,
                    "cue.called",
                    Some(&reference),
                    Some(&label),
                    json!({ "entry": format!("{entry:?}"), "cause": format!("{cause:?}") }),
                );
            }
            ShowEvent::Preload { slot } => {
                rec.point(
                    Level::Info,
                    "slot.preload",
                    Some(&format!("slot{slot}")),
                    None,
                    Value::Null,
                );
            }
            ShowEvent::Handover { from, to, fade_s } => rec.point(
                Level::Info,
                "handover",
                Some(&format!("slot{from}->slot{to}")),
                None,
                json!({ "fade_s": fade_s }),
            ),
            ShowEvent::SlotLoaded { slot, previous } => rec.point(
                Level::Info,
                "slot.loaded",
                Some(&format!("slot{slot}")),
                None,
                json!({ "previous": previous }),
            ),
            ShowEvent::ShowDj { slot } => {
                rec.point(Level::Info, "dj.show", Some(&format!("slot{slot}")), None, Value::Null);
            }
            ShowEvent::DjLogo { slot } => {
                rec.point(Level::Info, "dj.logo", Some(&format!("slot{slot}")), None, Value::Null);
            }
            ShowEvent::Title(text) => rec.point(Level::Info, "title", None, Some(text), Value::Null),
            ShowEvent::Toggle { what, on } => rec.point(
                Level::Info,
                "toggle",
                Some(&format!("{what:?}")),
                None,
                json!({ "on": on }),
            ),
        }
    }
}

/// Ordres reçus (hors potards et curseurs, qui feraient des centaines de lignes par seconde).
fn log_actions(log: Res<DebugLog>, mut received: MessageReader<ControlAction>) {
    for ControlAction(timed) in received.read() {
        let action = &timed.action;
        if action.family() == ActionFamily::Continuous {
            continue;
        }
        let name = format!("{action:?}");
        let ev = if name.starts_with("Trigger") {
            "world.trigger"
        } else {
            "action"
        };
        log.0
            .point(Level::Info, ev, Some(&name), None, json!({ "t": timed.time_s }));
    }
}

/// État mémorisé d'une image à l'autre par [`pulse`].
#[derive(Default)]
struct PulseState {
    since_pulse_s: f64,
    since_slow_s: f64,
    track: Option<usize>,
    underruns: u64,
    device_errors: u64,
}

/// Une fois par image : signal de vie du watchdog et images lentes. Une fois par seconde : battement de cœur.
#[allow(clippy::too_many_arguments)] // Paramètres de système Bevy : chacun est une ressource distincte.
fn pulse(
    log: Res<DebugLog>,
    time: Res<Time<Real>>,
    clock: Res<SimClock>,
    control: Res<SimControl>,
    show: Option<Res<CurrentShow>>,
    sheet: Option<Res<ShowSheet>>,
    audio: Option<Res<AudioStatus>>,
    snapshot: Option<Res<DiagnosticsSnapshot>>,
    mut state: Local<PulseState>,
) {
    let rec = &log.0;
    rec.tick(clock.frame(), clock.sim_ms());
    let dt = time.delta_secs();
    state.since_pulse_s += f64::from(dt);
    state.since_slow_s += f64::from(dt);

    if dt * 1000.0 > SLOW_FRAME_MS && state.since_slow_s >= SLOW_FRAME_REPEAT_S {
        state.since_slow_s = 0.0;
        rec.point(
            Level::Warn,
            "perf.slow_frame",
            None,
            None,
            json!({ "ms": (dt * 1000.0).round() }),
        );
    }

    let demo = audio.as_deref().and_then(|a| a.demo.as_ref());
    if let Some(demo) = demo {
        if demo.track != state.track {
            state.track = demo.track;
            rec.point(Level::Info, "audio.track", None, None, json!({ "track": demo.track }));
        }
        if demo.underruns > state.underruns {
            rec.point(
                Level::Warn,
                "audio.underrun",
                None,
                None,
                json!({ "new": demo.underruns - state.underruns, "total": demo.underruns }),
            );
            state.underruns = demo.underruns;
        }
        if demo.device_errors > state.device_errors {
            rec.point(
                Level::Error,
                "audio.device_error",
                None,
                None,
                json!({ "new": demo.device_errors - state.device_errors, "total": demo.device_errors }),
            );
            state.device_errors = demo.device_errors;
        }
    }

    if state.since_pulse_s < PULSE_PERIOD_S {
        return;
    }
    state.since_pulse_s = 0.0;
    let cue = show.as_deref().map(|s| {
        let (reference, label) = cue_names(sheet.as_deref(), s.slot, s.cue);
        json!({ "ref": reference, "label": label, "elapsed_s": s.cue_elapsed })
    });
    rec.heartbeat(json!({
        "paused": control.paused(),
        "cue": cue,
        "fps": snapshot.as_deref().map(|s| s.fps),
        "frame_p99_ms": snapshot.as_deref().map(|s| s.frame_p99_ms),
        "audio": demo.map(|d| json!({
            "playing": d.playing, "track": d.track, "position_s": d.position_s,
            "underruns": d.underruns, "device_errors": d.device_errors,
            "analysis_dropped": d.analysis_dropped,
        })),
    }));
}
