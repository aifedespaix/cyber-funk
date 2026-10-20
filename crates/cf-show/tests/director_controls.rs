//! Contrôles et sécurité de bout en bout : un bouton à maintien tenu pendant une passation de DJ (changement de profil
//! MIDI) est relâché proprement, et le noir glisse toujours, quelle que soit la suite d'ordres.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use cf_control::{Action, ActionTemplate, Binding, ControlProfile, InputSource, Mapper, Mode, RawEvent, RawInput};
use cf_control::{PortId, Switch};
use cf_show::director::ShowEvent;
use cf_show::limiter::{MAX_STEP, MIN_FADE};
use director_rig::{DT, Music, Rig, cue, event, slot};
use proptest::prelude::*;

fn pad(note: u8, action: ActionTemplate) -> Binding {
    Binding {
        source: InputSource::MidiNote {
            port: None,
            channel: 0,
            note,
        },
        action,
        mode: Mode::Momentary,
    }
}

fn hit(time_s: f64, note: u8, velocity: u8) -> RawInput {
    RawInput {
        time_s,
        event: RawEvent::MidiNote {
            port: PortId(0),
            channel: 0,
            note,
            velocity,
        },
    }
}

/// Le DJ A tient le pad 40 (Blackout à maintien) pendant que la passation charge le profil du DJ B, où ce même pad
/// coupe les flashs. Le noir s'éteint au changement de profil (en fondu), et le relâchement tardif du pad ne coupe pas
/// les flashs du nouveau profil.
#[test]
fn a_blackout_held_through_a_handover_is_released_cleanly() {
    let cues = cue("c0", "niveau-1-ville", "Cut", "Manual");
    let show = event(&format!(
        "{}{}",
        slot("a", "Semi", "", &cues),
        slot("b", "Semi", "", &cues)
    ));
    let profile_a = ControlProfile {
        name: "dj-a".into(),
        bindings: vec![pad(40, ActionTemplate::Blackout)],
    };
    let profile_b = ControlProfile {
        name: "dj-b".into(),
        bindings: vec![pad(40, ActionTemplate::FlashKill)],
    };
    let (mut mapper, _) = Mapper::from_profiles(&[&profile_a]).unwrap();
    let mut rig = Rig::new(&show, Music::Silent);

    // Le pad s'enfonce : noir.
    let on = mapper.map(&hit(rig.now, 40, 100)).unwrap();
    assert_eq!(on.action, Action::Blackout(Switch::On));
    rig.act(on.action);
    rig.run(1.0);
    assert!(rig.state().blackout_on);
    assert!((rig.state().blackout - 1.0).abs() < f32::EPSILON);

    // Passation, pad toujours tenu. Le binaire applique `SlotLoaded` comme en production : `Mapper::reload`.
    rig.act(Action::NextDj);
    let mut released = Vec::new();
    let mut reloaded = false;
    for _ in 0..600 {
        rig.frame();
        for e in rig.take_events() {
            if let ShowEvent::SlotLoaded { slot: 1, .. } = e {
                mapper.reload(&[&profile_b], rig.now, &mut released).unwrap();
                reloaded = true;
            }
        }
        for t in released.drain(..) {
            rig.act(t.action);
        }
        if reloaded {
            break;
        }
    }
    assert!(reloaded, "le créneau B est chargé");
    assert_eq!(rig.state().slot, 1);
    rig.frame();
    assert!(!rig.state().blackout_on, "aucun noir « allumé » ne survit au changement de profil");
    assert!(rig.state().blackout > 0.9, "il s'éteint en fondu, pas d'un coup");
    rig.run(1.0);
    assert!(rig.state().blackout.abs() < f32::EPSILON);

    // Le doigt quitte enfin le pad : le profil B n'a pas vu l'appui, rien ne se passe (les flashs restent permis).
    assert_eq!(mapper.map(&hit(rig.now, 40, 0)), None);
    assert!(rig.state().flashes);
}

#[derive(Debug, Clone, Copy)]
enum Order {
    On,
    Off,
    Toggle,
    Nothing,
}

fn order() -> impl Strategy<Value = Order> {
    prop_oneof![Just(Order::On), Just(Order::Off), Just(Order::Toggle), Just(Order::Nothing)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Pour toute suite d'ordres de noir et toute cadence d'images (trous compris), le noir ne bouge jamais plus vite
    /// qu'une course complète en `MIN_FADE`, ni à l'entrée ni à la sortie.
    #[test]
    fn blackout_never_moves_faster_than_min_fade(
        steps in proptest::collection::vec((order(), 0.0f32..0.2), 1..200),
    ) {
        let cues = cue("c0", "niveau-1-ville", "Cut", "Manual");
        let mut rig = Rig::new(&event(&slot("a", "Manual", "", &cues)), Music::Silent);
        let mut previous = rig.state().blackout;
        for (order, extra) in steps {
            // Tiré depuis 0 puis décalé (proptest 1.12 et les plages f32 qui ne partent pas de 0).
            let dt = DT * 0.25 + extra;
            match order {
                Order::On => rig.act(Action::Blackout(Switch::On)),
                Order::Off => rig.act(Action::Blackout(Switch::Off)),
                Order::Toggle => rig.act(Action::Blackout(Switch::Toggle)),
                Order::Nothing => {}
            }
            rig.frame_with(dt);
            let b = rig.state().blackout;
            prop_assert!((0.0..=1.0).contains(&b));
            prop_assert!((b - previous).abs() <= dt.min(MAX_STEP) / MIN_FADE + 1e-5, "{previous} → {b} en {dt} s");
            previous = b;
        }
    }
}
