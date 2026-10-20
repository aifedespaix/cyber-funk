//! Scénarios du chef du spectacle : modes Auto/Semi/Manuel, GO, Hold, attente du drop (`MAX_LATE`), entrées de cue,
//! passation de DJ, repli, ordres ponctuels, déterminisme.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use cf_control::{Action, CueIndex, DjIndex, Level, MacroIndex, Switch};
use cf_show::director::{
    CueCause, HandoverPhase, MAX_LATE, NO_BEAT_FALLBACK, ORDER_TTL, Order, ShowEvent, ShowToggle, bar_seconds,
    look_level,
};
use cf_show::load::LoadedShow;
use cf_show::model::{CueEntry, PresetTheme, SlotMode, Theme};
use director_rig::{DT, Music, Rig, cue, event, slot};

/// Trois cues : le premier avance après 10 s, le deuxième entre au drop et avance après 30 s.
fn three_cues(mode: &str) -> String {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "After(seconds: 10.0)"),
        cue("c1", "niveau-2-rues", "Cut", "After(seconds: 30.0)"),
        cue("c2", "niveau-3-envol", "Cut", "Manual"),
    ]
    .concat();
    slot("a", mode, "", &cues)
}

fn entered(events: &[ShowEvent]) -> Vec<(usize, usize, CueCause)> {
    events
        .iter()
        .filter_map(|e| match e {
            ShowEvent::CueEntered { slot, cue, cause, .. } => Some((*slot, *cue, *cause)),
            _ => None,
        })
        .collect()
}

#[test]
fn opening_loads_the_first_slot_and_enters_its_first_cue() {
    let rig = Rig::new(&event(&three_cues("Semi")), Music::Silent);
    assert_eq!(rig.state().slot, 0);
    assert_eq!(rig.state().cue, 0);
    assert_eq!(rig.state().mode, SlotMode::Semi);
    assert_eq!(rig.state().level, 1);
    assert!(matches!(rig.events[0], ShowEvent::SlotLoaded { slot: 0, .. }));
    assert_eq!(entered(&rig.events), [(0, 0, CueCause::Open)]);
}

#[test]
fn semi_without_go_advances_alone() {
    let mut rig = Rig::new(&event(&three_cues("Semi")), Music::Silent);
    rig.take_events();
    rig.run(9.9);
    assert_eq!(rig.state().cue, 0);
    let left = rig.state().next_cue_in.unwrap();
    assert!(left > 0.0 && left < 0.2, "{left}");
    rig.run(0.2);
    assert_eq!(rig.state().cue, 1);
    assert_eq!(entered(&rig.take_events()), [(0, 1, CueCause::Auto)]);
}

#[test]
fn go_before_the_deadline_advances_and_rearms() {
    let mut rig = Rig::new(&event(&three_cues("Semi")), Music::Silent);
    rig.run(4.0);
    rig.take_events();
    rig.act(Action::Go);
    rig.frame();
    assert_eq!(rig.state().cue, 1);
    assert_eq!(entered(&rig.take_events()), [(0, 1, CueCause::Go)]);
    // Le chronomètre du cue 1 part du GO : 30 s pleines, pas 30 s depuis l'échéance abandonnée du cue 0.
    let left = rig.state().next_cue_in.unwrap();
    assert!((left - 30.0).abs() < 0.1, "{left}");
    rig.run(29.8);
    assert_eq!(rig.state().cue, 1, "l'ancienne échéance (10 s) ne joue plus");
    rig.run(0.3);
    assert_eq!(rig.state().cue, 2);
}

#[test]
fn auto_behaves_like_semi_and_manual_never_advances_alone() {
    let mut auto = Rig::new(&event(&three_cues("Auto")), Music::Silent);
    auto.run(10.1);
    assert_eq!(auto.state().cue, 1);

    let mut manual = Rig::new(&event(&three_cues("Manual")), Music::Silent);
    manual.run(120.0);
    assert_eq!(manual.state().cue, 0);
    assert_eq!(manual.state().next_cue_in, None, "en Manuel, aucune échéance n'est annoncée");
    manual.act(Action::Go);
    manual.frame();
    assert_eq!(manual.state().cue, 1);
}

#[test]
fn hold_never_advances_but_go_still_works() {
    let mut rig = Rig::new(&event(&three_cues("Auto")), Music::Silent);
    rig.act(Action::Hold(Switch::On));
    rig.run(3600.0);
    assert_eq!(rig.state().cue, 0);
    assert!(rig.state().hold);
    assert!(rig.state().pending.is_none());
    rig.act(Action::Go);
    rig.frame();
    assert_eq!(rig.state().cue, 1);
    rig.run(600.0);
    assert_eq!(rig.state().cue, 1, "toujours figé");
    rig.act(Action::Hold(Switch::Off));
    rig.frame();
    rig.frame();
    assert_eq!(rig.state().cue, 2, "relâché : l'échéance passée appelle le suivant");
}

#[test]
fn hold_cancels_an_automatic_call_waiting_for_its_drop() {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "After(seconds: 5.0)"),
        cue("c1", "niveau-2-rues", "OnDrop", "Manual"),
    ]
    .concat();
    let mut rig = Rig::new(&event(&slot("a", "Auto", "", &cues)), Music::Kicks(120.0));
    rig.run(6.0);
    assert!(rig.state().pending.is_some_and(|p| p.cause == CueCause::Auto));
    rig.act(Action::Hold(Switch::On));
    rig.frame();
    assert!(rig.state().pending.is_none());
    rig.drop_next();
    rig.frame();
    assert_eq!(rig.state().cue, 0, "figé : le drop ne fait rien entrer");
}

/// Le cue 1 attend un drop qui ne vient pas : il part à la première phrase après `MAX_LATE`.
#[test]
fn a_late_drop_leaves_at_max_late_on_the_next_phrase() {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "After(seconds: 10.0)"),
        cue("c1", "niveau-2-rues", "OnDrop", "Manual"),
    ]
    .concat();
    let mut rig = Rig::new(&event(&slot("a", "Auto", "", &cues)), Music::Kicks(120.0));
    rig.run(10.1);
    let pending = rig.state().pending.unwrap();
    assert_eq!((pending.cue, pending.entry), (1, CueEntry::OnDrop));
    assert_eq!(rig.state().cue, 0, "en attente du drop");
    let waited = rig.run_until(200.0, |s| s.cue == 1).unwrap();
    // Une phrase de 16 temps à 120 BPM dure 8 s.
    let phrase = 16.0 * 60.0 / 120.0;
    assert!(
        waited > f64::from(MAX_LATE) && waited <= f64::from(MAX_LATE) + phrase + 0.1,
        "entré après {waited} s d'attente"
    );
}

#[test]
fn a_late_drop_without_beats_leaves_after_the_fallback_and_a_drop_in_time_enters_on_it() {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "After(seconds: 1.0)"),
        cue("c1", "niveau-2-rues", "OnDrop", "After(seconds: 30.0)"),
        cue("c2", "niveau-3-envol", "OnDrop", "Manual"),
    ]
    .concat();
    let mut rig = Rig::new(&event(&slot("a", "Auto", "", &cues)), Music::Breakdown);
    rig.run(1.1);
    let called = rig.state().pending.unwrap().waited;
    let waited = rig.run_until(300.0, |s| s.cue == 1).unwrap() + f64::from(called);
    let fallback = f64::from(MAX_LATE + NO_BEAT_FALLBACK);
    assert!(waited > fallback && waited < fallback + 0.05, "{waited}");

    // Le cue 2 est dû 30 s après l'appel du cue 1 (pas après son entrée) ; un drop 5 s plus tard le fait entrer.
    let mut rig = Rig::new(&event(&slot("a", "Auto", "", &cues)), Music::Kicks(124.0));
    rig.run(1.1);
    let called = f64::from(rig.state().pending.unwrap().waited);
    rig.run(20.0);
    rig.drop_next();
    rig.frame();
    assert_eq!(rig.state().cue, 1, "le drop fait entrer le cue 1, appelé il y a 20 s");
    let waited = called + 20.0 + f64::from(DT);
    assert!(
        (rig.state().cue_elapsed - waited).abs() < 1e-3,
        "le chronomètre part de l'appel : {}",
        rig.state().cue_elapsed
    );
    let due = rig.run_until(30.0, |s| s.pending.is_some()).unwrap();
    assert!(
        (due - (30.0 - waited)).abs() < 0.02,
        "dû 30 s après l'appel, pas 30 s après l'entrée : {due}"
    );
    rig.run(5.0);
    rig.drop_next();
    rig.frame();
    assert_eq!(rig.state().cue, 2);
}

#[test]
fn entries_cut_on_a_downbeat_fade_at_once_and_wait_for_the_phrase() {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "Manual"),
        cue("c1", "niveau-2-rues", "Cut", "Manual"),
        cue("c2", "niveau-3-envol", "Fade(bars: 2)", "Manual"),
        cue("c3", "niveau-4-meteores", "OnPhrase", "Manual"),
    ]
    .concat();
    let mut rig = Rig::new(&event(&slot("a", "Manual", "", &cues)), Music::Kicks(120.0));
    rig.run(0.6);
    rig.act(Action::Go);
    let waited = rig.run_until(4.0, |s| s.cue == 1).unwrap();
    assert!(waited <= 2.0 + 0.05, "une coupe attend au plus une mesure : {waited}");
    assert_eq!(rig.audio.count % 4, 0, "la coupe tombe sur un temps fort");

    rig.act(Action::Go);
    rig.frame();
    assert_eq!(rig.state().cue, 2, "un fondu commence tout de suite");

    rig.act(Action::Go);
    let waited = rig.run_until(10.0, |s| s.cue == 3).unwrap();
    assert!(waited <= 8.0 + 0.05);
    assert_eq!(rig.audio.count % 16, 0, "l'entrée tombe sur une phrase");
}

#[test]
fn a_second_go_enters_at_once_and_back_cancels_a_call() {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "Manual"),
        cue("c1", "niveau-2-rues", "OnDrop", "Manual"),
    ]
    .concat();
    let mut rig = Rig::new(&event(&slot("a", "Semi", "", &cues)), Music::Kicks(120.0));
    rig.act(Action::Go);
    rig.frame();
    assert!(rig.state().pending.is_some());
    rig.act(Action::Back);
    rig.frame();
    assert!(rig.state().pending.is_none(), "BACK annule le GO en attente");
    assert_eq!(rig.state().cue, 0);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.frame();
    assert_eq!(rig.state().cue, 1, "second GO : tout de suite");
    rig.act(Action::Back);
    rig.frame();
    assert_eq!(rig.state().cue, 0);
    rig.act(Action::GotoCue(CueIndex(1)));
    rig.act(Action::GotoCue(CueIndex(9)));
    rig.frame();
    assert_eq!(rig.state().cue, 1, "GotoCue entre tout de suite, un rang hors liste est ignoré");
}

#[test]
fn handover_loads_the_next_slot_first_cue_and_profile() {
    let a = three_cues("Semi");
    let b_cues = [
        r#"Cue(id: "b0", name: "Arrivée", look: "niveau-3-envol", entry: OnPhrase, triggers: [DjLogo, Title("B")], advance: Bars(bars: 4)),"#.to_owned(),
        cue("b1", "niveau-4-meteores", "Cut", "Manual"),
    ]
    .concat();
    let b = slot("b", "Auto", ", theme: Preset(Synthwave)", &b_cues);
    let mut rig = Rig::new(&event(&format!("{a}{b}")), Music::Kicks(120.0));
    rig.run(1.0);
    rig.take_events();

    rig.act(Action::ArmDj(DjIndex(1)));
    rig.frame();
    assert_eq!(rig.state().armed_slot, Some(1));
    assert_eq!(rig.take_events(), [ShowEvent::Preload { slot: 1 }]);

    rig.act(Action::NextDj);
    let fading = rig.run_until(DOWNBEAT_WAIT, |s| {
        s.handover.is_some_and(|h| h.phase == HandoverPhase::Fade)
    });
    assert!(fading.is_some(), "le fondu part sur un temps fort");
    let bar = bar_seconds(Some(120.0));
    assert!(rig.events.iter().any(|e| matches!(
        e,
        ShowEvent::Handover { from: 0, to: 1, fade_s } if (fade_s - bar).abs() < 1e-4
    )));
    assert_eq!(*rig.state().theme, Theme::Preset(PresetTheme::Synthwave), "la palette glisse vers le thème du DJ");
    assert!((rig.state().theme_fade_s - bar).abs() < 1e-4, "sur une mesure");

    // Fin du fondu, puis le premier cue attend sa phrase ; à son entrée, le créneau est chargé.
    let loaded = rig.run_until(bar as f64 + 9.0, |s| s.slot == 1).unwrap();
    assert!(loaded > f64::from(bar) - 0.05);
    let s = rig.state();
    assert_eq!((s.slot, s.cue, s.mode, s.armed_slot), (1, 0, SlotMode::Auto, None));
    assert_eq!(rig.audio.count % 16, 0, "entrée à la phrase");
    let events = rig.take_events();
    let loaded_at = events
        .iter()
        .position(|e| *e == ShowEvent::SlotLoaded { slot: 1, previous: 0 })
        .unwrap();
    assert!(matches!(
        events[loaded_at + 1],
        ShowEvent::CueEntered {
            slot: 1,
            cue: 0,
            previous_slot: 0,
            previous_cue: 0,
            cause: CueCause::Handover
        }
    ));
    assert!(events.contains(&ShowEvent::DjLogo { slot: 1 }));
    assert!(events.contains(&ShowEvent::Title("B".into())));
    assert!(rig.state().handover.is_none());

    rig.act(Action::PrevDj);
    let back = rig.run_until(DOWNBEAT_WAIT + 12.0, |s| s.slot == 0);
    assert!(back.is_some(), "PrevDj revient au créneau précédent");
}

const DOWNBEAT_WAIT: f64 = 4.1;

#[test]
fn next_dj_without_arming_goes_to_the_next_slot_and_stops_at_the_last() {
    let mut rig = Rig::new(&event(&format!("{}{}", three_cues("Semi"), three_cues("Auto").replace("\"a\"", "\"b\""))), Music::Silent);
    rig.act(Action::NextDj);
    assert!(rig.run_until(5.0, |s| s.slot == 1).is_some());
    rig.act(Action::NextDj);
    rig.run(10.0);
    assert_eq!(rig.state().slot, 1, "pas de créneau après le dernier");
    assert!(rig.state().handover.is_none());
}

#[test]
fn blackout_fades_out_and_back_in() {
    let mut rig = Rig::new(&event(&three_cues("Manual")), Music::Silent);
    rig.act(Action::Blackout(Switch::On));
    rig.frame();
    let first = rig.state().blackout;
    assert!(first > 0.0 && first < 0.1, "le noir glisse, il ne coupe pas : {first}");
    let mut previous = first;
    let mut frames = 1;
    while rig.state().blackout < 1.0 {
        rig.frame();
        let b = rig.state().blackout;
        assert!(b - previous <= DT / cf_show::limiter::MIN_FADE + 1e-6);
        previous = b;
        frames += 1;
    }
    assert!(frames as f32 * DT >= cf_show::director::BLACKOUT_FADE - DT);
    assert!(rig.events.contains(&ShowEvent::Toggle {
        what: ShowToggle::Blackout,
        on: true
    }));
    rig.act(Action::Blackout(Switch::Toggle));
    rig.frame();
    assert!(rig.state().blackout < 1.0 && rig.state().blackout > 0.9, "la sortie glisse aussi");
    rig.run(1.0);
    assert!(rig.state().blackout.abs() < f32::EPSILON);
}

#[test]
fn safe_look_freezes_the_conduct_and_go_leaves_it() {
    let mut rig = Rig::new(&event(&three_cues("Auto")), Music::Silent);
    rig.act(Action::SafeLook);
    rig.run(30.0);
    let s = rig.state();
    assert!(s.safe_look);
    assert_eq!(s.cue, 0, "l'avancée automatique est figée");
    assert_eq!(s.level, 0, "niveau du look de secours (pré-intro)");
    assert!(s.gates().events.is_empty(), "listes du look de secours");
    assert!(s.gates().fx.flash.abs() < 1e-3);
    rig.take_events();
    rig.act(Action::Go);
    rig.frame();
    assert!(!rig.state().safe_look);
    // GO quitte le repli sans appeler de cue ; l'échéance passée du cue 0 reprend aussitôt ses droits.
    assert_eq!(entered(&rig.take_events()), [(0, 1, CueCause::Auto)]);

    let manual = three_cues("Manual");
    let mut rig = Rig::new(&event(&manual), Music::Silent);
    rig.act(Action::SafeLook);
    rig.frame();
    rig.act(Action::Go);
    rig.frame();
    assert!(!rig.state().safe_look);
    assert_eq!(rig.state().cue, 0, "GO quitte le repli sans avancer");
}

#[test]
fn macros_and_master_drive_the_current_look() {
    let cues = cue("c0", "ville-qui-monte", "Cut", "Manual");
    let mut rig = Rig::new(&event(&slot("a", "Manual", "", &cues)), Music::Silent);
    rig.run(10.0);
    // « Vitesse » : gates.cruise de 0,9 à 1,2. Libre, le potard montre la position qui reproduit le look (1,07).
    let knob = rig.state().macros[1];
    assert!(!knob.engaged);
    assert!((0.9 + 0.3 * knob.value - rig.state().gates().cruise).abs() < 1e-3);
    rig.act(Action::Macro {
        index: MacroIndex(1),
        value: Level::Set(1.0),
    });
    rig.run(20.0);
    assert!((rig.state().gates().cruise - 1.2).abs() < 1e-3);
    rig.act(Action::Macro {
        index: MacroIndex(7),
        value: Level::Set(1.0),
    });
    rig.frame();
    assert!(!rig.state().macros[7].engaged, "pas de 8e macro dans ce look");

    let flash = rig.state().gates().fx.flash;
    assert!(flash > 0.1);
    rig.act(Action::Master(Level::Set(0.0)));
    rig.frame();
    assert!(rig.state().master > 0.9, "l'intensité globale glisse");
    let after_one = rig.state().gates().fx.flash;
    assert!(flash - after_one < 0.05, "les portes glissent, elles ne sautent pas");
    rig.run(30.0);
    assert!(rig.state().master.abs() < f32::EPSILON);
    assert!(rig.state().gates().fx.flash < 1e-3);
    assert!((rig.state().gates().cruise - 1.2).abs() < 1e-3, "l'allure n'est pas une intensité");
}

#[test]
fn one_shot_orders_are_taken_once_and_expire() {
    let mut rig = Rig::new(&event(&three_cues("Manual")), Music::Silent);
    rig.act(Action::SkipShot);
    rig.act(Action::ShowDj);
    rig.frame();
    assert!(rig.state().orders.pending(Order::SkipShot));
    assert!(rig.director.take(Order::SkipShot));
    assert!(!rig.director.take(Order::SkipShot));
    assert_eq!(rig.state().dj_show_seq, 1);
    rig.run(f64::from(ORDER_TTL) + 0.1);
    assert!(!rig.director.take(Order::ShowDj), "personne ne l'a pris : expiré");
    rig.act(Action::Trigger(cf_control::TriggerId::new("storm")));
    rig.frame();
    assert_eq!(rig.director.take_trigger().map(|id| id.0.to_string()), Some("storm".to_owned()));
}

#[test]
fn flash_kill_and_breathing_toggle_and_reach_the_drop() {
    let mut rig = Rig::new(&event(&three_cues("Manual")), Music::Silent);
    assert!(rig.state().flashes);
    rig.act(Action::FlashKill(Switch::On));
    rig.act(Action::Breathing(Switch::Toggle));
    rig.act(Action::ForceDrop);
    rig.frame();
    assert!(!rig.state().flashes && rig.state().breathing);
    assert!(rig.state().drop_now, "drop forcé, sur l'image même");
    assert!(rig.director.drop_plan().is_some_and(|p| p.fx.flash.abs() < f32::EPSILON && p.forced));
}

/// Même graine, mêmes entrées : même spectacle (drops, cues, chaleur), image par image.
#[test]
fn same_seed_same_show() {
    let trace = |seed| {
        let cues = [
            cue("c0", "niveau-3-envol", "Cut", "Bars(bars: 8)"),
            cue("c1", "niveau-5-cosmos", "OnDrop", "OnDrop"),
            cue("c2", "niveau-4-meteores", "Fade(bars: 4)", "Manual"),
        ]
        .concat();
        let mut rig = Rig::with_seed(&event(&slot("a", "Auto", "", &cues)), Music::Kicks(128.0), seed);
        let mut out = Vec::new();
        for i in 0..6000 {
            if i % 900 == 450 {
                rig.drop_next();
            }
            rig.frame();
            let s = rig.state();
            let plan = rig.director.drop_plan().map(|p| (p.shot.clone(), p.stunt.clone(), p.fx.burst));
            out.push((s.cue, s.cue_seq, s.heat.to_bits(), s.drop_now, plan));
        }
        out
    };
    assert_eq!(trace(42), trace(42));
}

/// Le niveau déduit des portes reproduit les six niveaux de Sillage (et range « La ville qui monte » au niveau 3).
#[test]
fn levels_of_the_six_sillage_looks() {
    let library = director_rig::library();
    let mut checked = 0;
    for (id, entry) in library.iter() {
        let expected = match id.as_str() {
            "ville-qui-monte" => 3,
            other => other
                .strip_prefix("niveau-")
                .and_then(|rest| rest.get(..1))
                .and_then(|digit| digit.parse::<u8>().ok())
                .unwrap_or_else(|| panic!("look inattendu {other}")),
        };
        assert_eq!(look_level(&entry.look.gates), expected, "{id}");
        checked += 1;
    }
    assert!(checked >= 7);
}

/// La montée automatique : la pré-intro attend la musique (`OnMusic` : au moins 45 s, puis ~8 s d'énergie), puis
/// le niveau 1 entre en coupe.
#[test]
fn the_auto_climb_leaves_the_pre_intro_when_the_music_is_there() {
    let show = LoadedShow::load(&director_rig::assets().join("shows/montee-auto.ron"), &director_rig::library()).unwrap();
    let mut rig = Rig::new(&show, Music::Silent);
    rig.run(120.0);
    assert_eq!((rig.state().cue, rig.state().level), (0, 0), "sans musique, la pré-intro attend");
    rig.music = Music::Kicks(124.0);
    rig.audio.intensity = 0.7;
    let waited = rig.run_until(60.0, |s| s.cue == 1).unwrap();
    assert!((8.0..10.5).contains(&waited), "{waited}");
    assert_eq!(rig.state().level, 1);
    assert!(rig.state().next_cue_in.is_some_and(|s| s > 640.0));
}
