//! J2-07 : sauvegarde continue atomique, reprise, fenêtre de 15 min, fichiers d'événement et copies de secours.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use std::fs;

use cf_control::{Action, DjIndex, Level, MacroIndex};
use cf_show::director::{CueCause, ShowDirector, ShowEvent, ShowInput};
use cf_show::load::LoadedShow;
use cf_show::persist::{
    EventDocument, PersistError, RESUME_WINDOW_S, SAVE_EVERY_S, SavePacer, ShowSave, backup_path, clear_save,
    read_save, write_atomic, write_save,
};
use director_rig::{DT, Music, Rig, cue, event, library, slot, titles};
use rand_pcg::Pcg32;

const NOW: f64 = 1_800_000_000.0;

/// Deux créneaux : le premier avance seul (10 s, 30 s), le second attend.
fn two_slots() -> LoadedShow {
    let a = [
        cue("a0", "niveau-1-ville", "Cut", "After(seconds: 10.0)"),
        cue("a1", "ville-qui-monte", "Cut", "After(seconds: 600.0)"),
    ]
    .concat();
    let b = cue("b0", "niveau-3-envol", "Cut", "Manual");
    event(&format!("{}{}", slot("a", "Auto", "", &a), slot("b", "Manual", "", &b)))
}

/// Un set joué 42 s : cue 1 depuis 32 s, macro 1 touchée, DJ 1 armé.
fn played() -> (LoadedShow, Rig) {
    let show = two_slots();
    let mut rig = Rig::with_seed(&show, Music::Silent, 0xC0FF_EE42);
    rig.run(10.0);
    rig.act(Action::Macro {
        index: MacroIndex(1),
        value: Level::Set(0.75),
    });
    rig.act(Action::ArmDj(DjIndex(1)));
    rig.run(32.0);
    (show, rig)
}

fn capture(rig: &Rig, at: f64) -> ShowSave {
    ShowSave::capture("Banc d'essai", rig.state(), rig.director.seed(), at)
}

#[test]
fn interrupted_write_leaves_the_previous_save_readable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etat.ron");
    let (_, rig) = played();
    let good = capture(&rig, NOW);
    write_save(&path, &good).unwrap();

    // Coupure en pleine écriture : un `.tmp` voisin à moitié écrit, jamais renommé.
    let text = good.to_ron().unwrap();
    fs::write(dir.path().join("etat.ron.tmp"), &text[..text.len() / 2]).unwrap();
    assert_eq!(read_save(&path, NOW + 1.0).unwrap(), Some(good.clone()));

    // La coupure a eu lieu avant la toute première sauvegarde : rien à reprendre, sans erreur.
    let fresh = dir.path().join("neuf.ron");
    fs::write(dir.path().join("neuf.ron.tmp"), "Show").unwrap();
    assert_eq!(read_save(&fresh, NOW).unwrap(), None);

    // L'écriture suivante remplace l'orphelin et ne laisse rien derrière elle.
    let mut later = good;
    later.saved_at_s = NOW + 5.0;
    write_save(&path, &later).unwrap();
    assert_eq!(read_save(&path, NOW + 6.0).unwrap(), Some(later));
    assert!(!dir.path().join("etat.ron.tmp").exists());
}

#[test]
fn failed_write_keeps_the_old_file_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etat.ron");
    write_atomic(&path, b"ancien").unwrap();
    // Le renommage échoue (la cible est devenue un dossier) : erreur remontée, pas de `.tmp` abandonné.
    let blocked = dir.path().join("bloque");
    fs::create_dir(&blocked).unwrap();
    assert!(matches!(write_atomic(&blocked, b"x"), Err(PersistError::Io { .. })));
    assert!(!dir.path().join("bloque.tmp").exists());
    assert_eq!(fs::read(&path).unwrap(), b"ancien");
}

#[test]
fn resume_restores_cue_elapsed_time_seed_and_knobs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etat.ron");
    let (show, rig) = played();
    assert_eq!(rig.state().cue, 1);
    write_save(&path, &capture(&rig, NOW)).unwrap();

    // Le programme redémarre 40 s plus tard.
    let save = read_save(&path, NOW + 40.0).unwrap().unwrap();
    let mut resumed = ShowDirector::<Pcg32>::resume_from(&show, titles(), &save).unwrap();
    // Lecture avant la première image (interface de reprise) : le temps du cue est déjà publié.
    assert!((resumed.state().cue_elapsed - rig.state().cue_elapsed).abs() <= 5.0);
    // Une image : les portes se posent d'un coup (pas de fondu depuis un état vide), macros comprises.
    let input = ShowInput {
        dt: DT,
        now: 0.0,
        local_time_s: 0.0,
        audio: &rig.audio,
        real_drop: false,
        actions: &[],
    };
    resumed.update(&input);
    let (before, after) = (rig.state(), resumed.state());
    assert_eq!((after.slot, after.cue), (before.slot, before.cue));
    assert!((after.cue_elapsed - before.cue_elapsed).abs() <= 5.0);
    assert!((after.elapsed - before.elapsed).abs() <= 5.0);
    assert_eq!(resumed.seed(), rig.director.seed());
    assert_eq!(after.armed_slot, Some(1));
    assert_eq!(after.macros[1].value.to_bits(), before.macros[1].value.to_bits());
    assert!(after.macros[1].engaged);
    assert!(
        (after.gates().cruise - before.gates().cruise).abs() < 1e-3,
        "portes posées sans fondu"
    );
    let events: Vec<_> = resumed.drain_events().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        ShowEvent::CueEntered {
            cue: 1,
            cause: CueCause::Open,
            ..
        }
    )));
}

#[test]
fn resume_picks_the_right_slot_and_refuses_foreign_or_stale_saves() {
    let (show, rig) = played();
    let mut save = capture(&rig, NOW);
    save.slot = 1;
    save.cue = 0;
    save.armed_slot = Some(1); // le créneau en cours ne peut pas être « armé » : ignoré
    let resumed = ShowDirector::<Pcg32>::resume_from(&show, titles(), &save).unwrap();
    assert_eq!((resumed.state().slot, resumed.state().cue), (1, 0));
    assert_eq!(resumed.state().armed_slot, None);

    let mut other = save.clone();
    other.event_name = "Autre soirée".into();
    assert!(matches!(
        ShowDirector::<Pcg32>::resume_from(&show, titles(), &other),
        Err(PersistError::WrongEvent { .. })
    ));
    let mut gone = save;
    gone.cue = 9;
    assert!(matches!(
        ShowDirector::<Pcg32>::resume_from(&show, titles(), &gone),
        Err(PersistError::OutOfRange { slot: 1, cue: 9 })
    ));
}

#[test]
fn resume_window_is_fifteen_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etat.ron");
    let (_, rig) = played();
    write_save(&path, &capture(&rig, NOW)).unwrap();
    assert!(read_save(&path, NOW + RESUME_WINDOW_S - 1.0).unwrap().is_some());
    assert!(read_save(&path, NOW + RESUME_WINDOW_S + 1.0).unwrap().is_none());
    assert!(
        read_save(&path, NOW - 1.0).unwrap().is_none(),
        "datée du futur : horloge reculée"
    );
    clear_save(&path).unwrap();
    clear_save(&path).unwrap();
    assert!(read_save(&path, NOW).unwrap().is_none());
}

#[test]
fn corrupt_or_unknown_saves_are_reported_not_panicked_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etat.ron");
    let (_, rig) = played();
    let good = capture(&rig, NOW).to_ron().unwrap();
    for bad in [
        good[..good.len() / 2].to_owned(),
        good.replace("version: 1", "version: 99"),
        good.replace("elapsed_s:", "elapsed_s: -"),
        String::new(),
    ] {
        fs::write(&path, &bad).unwrap();
        assert!(matches!(read_save(&path, NOW), Err(PersistError::Corrupt(_))), "{bad}");
    }
}

#[test]
fn pacer_fires_every_five_seconds() {
    let mut pacer = SavePacer::default();
    let fires = (0..60 * 60).filter(|_| pacer.tick(1.0 / 60.0)).count();
    assert!(
        (11..=12).contains(&fires),
        "{fires} sauvegardes en 60 s (une toutes les {SAVE_EVERY_S} s)"
    );
    assert!(!pacer.tick(f32::NAN));
}

#[test]
fn event_documents_save_as_back_up_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("soiree.cfshow");
    let (mut doc, _) = {
        let show = two_slots();
        (EventDocument::new(show.event().clone()), show)
    };
    assert!(matches!(doc.save(), Err(PersistError::NoPath)));

    doc.save_as(&path).unwrap();
    assert_eq!(doc.path(), Some(path.as_path()));
    assert!(
        !backup_path(&path).exists(),
        "rien à sauvegarder avant le premier enregistrement"
    );
    let first = fs::read_to_string(&path).unwrap();

    // Modification puis « enregistrer » : l'ancienne version passe dans le `.bak`.
    doc.event_mut().settings.name = "Renommée".into();
    doc.save().unwrap();
    assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), first);
    let (reopened, show) = EventDocument::open(&path, &library()).unwrap();
    assert_eq!(reopened.event(), doc.event());
    assert_eq!(show.event().settings.name, "Renommée");

    // « Enregistrer sous » un autre nom : le fichier associé change, l'ancien reste tel quel.
    let other = dir.path().join("copie.cfshow");
    doc.save_as(&other).unwrap();
    assert_eq!(doc.path(), Some(other.as_path()));
    assert_eq!(
        EventDocument::open(&path, &library()).unwrap().0.event(),
        reopened.event()
    );

    // Un enregistrement qui échoue ne change pas le fichier associé.
    let bad = dir.path().join("absent").join("x.cfshow");
    assert!(doc.save_as(&bad).is_err());
    assert_eq!(doc.path(), Some(other.as_path()));
}
