//! `Démarrer` repart avec le DJ désigné comme actuel : le chef du spectacle entre dans le premier cue de ce créneau.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;

use cf_show::director::{ShowDirector, ShowEvent};
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use rand_pcg::Pcg32;

fn demo() -> LoadedShow {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = LookLibrary::load_dir(&assets.join("looks")).expect("looks");
    LoadedShow::load(&assets.join("shows/demo.ron"), &looks).expect("show")
}

#[test]
fn the_default_director_starts_on_the_first_slot() {
    let director = ShowDirector::<Pcg32>::new(&demo(), DropTitles::empty(), 1).expect("chef");
    assert_eq!(director.state().slot, 0);
}

#[test]
fn a_director_started_at_a_slot_enters_its_first_cue() {
    let show = demo();
    assert!(
        show.event().slots.len() >= 2,
        "le show de démonstration a au moins deux DJs"
    );
    let mut director = ShowDirector::<Pcg32>::new_at(&show, DropTitles::empty(), 1, 1).expect("chef");
    assert_eq!((director.state().slot, director.state().cue), (1, 0));
    let events: Vec<ShowEvent> = director.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ShowEvent::SlotLoaded { slot: 1, previous: 1 })),
        "{events:?}"
    );
}

#[test]
fn a_start_slot_outside_the_list_falls_back_to_the_first_slot() {
    let director = ShowDirector::<Pcg32>::new_at(&demo(), DropTitles::empty(), 1, 99).expect("chef");
    assert_eq!(director.state().slot, 0);
}
