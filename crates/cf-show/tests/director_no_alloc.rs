//! Aucune allocation par image en régime établi (règle 4) : le chef du spectacle tourne sous `assert_no_alloc`, qui
//! remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime
//! couvre les kicks, le glissement d'acte, et toutes les actions continues, bascules et ordres ponctuels de
//! l'opérateur. Restent hors zone, parce qu'ils allouent une fois par événement et non par image : la préparation
//! d'un drop (copie d'identifiants, voir `drop_no_alloc.rs`) et l'entrée d'un cue dont les listes sont plus longues
//! que celles déjà en mémoire.

#![allow(
    clippy::unwrap_used,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_analysis::AudioFeatures;
use cf_control::{Action, DjIndex, Layer, Level, MacroIndex, Switch, TitleId, TriggerId};
use cf_show::director::{Order, ShowDirector, ShowInput};
use director_rig::{DT, cue, event, slot, titles};
use rand_pcg::Pcg32;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn steady_state_frames_never_allocate() {
    let cues = [
        cue("c0", "ville-qui-monte", "Cut", "After(seconds: 10000.0)"),
        cue("c1", "niveau-5-cosmos", "Cut", "Manual"),
    ]
    .concat();
    let other = cue("b0", "niveau-1-ville", "Cut", "Manual");
    let show = event(&format!(
        "{}{}",
        slot("a", "Semi", "", &cues),
        slot("b", "Semi", "", &other)
    ));
    let mut director: ShowDirector<Pcg32> = ShowDirector::new(&show, titles(), 3).unwrap();
    // Tout ce qui alloue est fait ici : actions construites d'avance (leurs identifiants sont partagés).
    let script = [
        Action::Macro {
            index: MacroIndex(0),
            value: Level::Shift(0.1),
        },
        Action::Macro {
            index: MacroIndex(2),
            value: Level::Set(0.8),
        },
        Action::Master(Level::Set(0.4)),
        Action::Master(Level::Shift(0.3)),
        Action::LayerFade {
            layer: Layer::Foreground,
            value: Level::Set(0.2),
        },
        Action::Blackout(Switch::On),
        Action::Blackout(Switch::Off),
        Action::Hold(Switch::Toggle),
        Action::FlashKill(Switch::Toggle),
        Action::Breathing(Switch::Toggle),
        Action::SkipShot,
        Action::ShowDj,
        Action::Ascension,
        Action::RandomEvent,
        Action::Trigger(TriggerId::new("storm")),
        Action::Title(TitleId::new("carton-libre")),
        Action::ArmDj(DjIndex(1)),
        Action::TapTempo,
    ];
    let mut audio = AudioFeatures {
        live: true,
        bpm: Some(128.0),
        intensity: 0.5,
        ..AudioFeatures::NEUTRAL
    };
    let mut actions: Vec<Action> = Vec::with_capacity(4);
    let mut now = 0.0;
    let mut frame = |i: u64, guarded: bool| {
        now += f64::from(DT);
        // Un kick tous les 28 pas ≈ 128 BPM à 60 i/s.
        audio.kick = i.is_multiple_of(28);
        if audio.kick {
            audio.count += 1;
        }
        audio.beat = f64::from(audio.count) + (i % 28) as f64 / 28.0;
        actions.clear();
        if i.is_multiple_of(20) {
            actions.push(script[(i / 20) as usize % script.len()].clone());
        }
        let input = ShowInput {
            dt: DT,
            now,
            local_time_s: 80_000.0,
            audio: &audio,
            real_drop: false,
            actions: &actions,
        };
        let mut step = || {
            director.update(&input);
            director.drain_events().for_each(drop);
            director.take(Order::SkipShot);
            director.take_trigger();
        };
        if guarded {
            assert_no_alloc(step);
        } else {
            step();
        }
    };
    // Rodage : chaque action une première fois, hors zone.
    for i in 0..(20 * script.len() as u64 + 60) {
        frame(i, false);
    }
    // Régime établi : 5 minutes d'images, dix passages du script.
    let start = 20 * script.len() as u64 + 60;
    for i in start..start + 18_000 {
        frame(i, true);
    }
}
