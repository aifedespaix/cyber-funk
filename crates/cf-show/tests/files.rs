//! Les fichiers livrés (`assets/looks/`, `assets/shows/demo.ron`) se chargent, se valident et se réécrivent à
//! l'identique.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::path::PathBuf;

use cf_show::load::{LoadedShow, LookLibrary, LookOrigin, event_to_ron, look_to_ron, parse_event, parse_look};
use cf_show::model::{CueAdvance, LookId, PaletteRef, SlotMode};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn library() -> LookLibrary {
    LookLibrary::load_dir(&assets().join("looks")).unwrap_or_else(|e| panic!("{e}"))
}

fn demo() -> LoadedShow {
    LoadedShow::load(&assets().join("shows/demo.ron"), &library()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn library_loads() {
    let library = library();
    let id = LookId::new("ville-qui-monte").unwrap();
    let entry = library.get(&id).expect("look d'exemple présent");
    assert_eq!(entry.look.name, "La ville qui monte");
    assert_eq!(entry.look.macros.len(), 3);
}

#[test]
fn demo_loads_with_two_slots() {
    let show = demo();
    let event = show.event();
    assert_eq!(event.slots.len(), 2);
    assert_eq!(event.slots[0].mode, SlotMode::Semi);
    assert_eq!(
        event.slots[1].cues[2].advance,
        CueAdvance::At {
            time: "01:00".to_owned().try_into().unwrap()
        }
    );

    // Le look embarqué « calme » vient de l'événement, « ville-qui-monte » de la bibliothèque.
    let calme = show.look(&LookId::new("calme").unwrap()).unwrap();
    assert_eq!(calme.origin, LookOrigin::Event { shadows: None });
    let ville = show.look(&LookId::new("ville-qui-monte").unwrap()).unwrap();
    assert!(matches!(ville.origin, LookOrigin::Library(_)));
}

#[test]
fn cue_overrides_apply() {
    let show = demo();
    let demesure = &show.event().slots[1].cues[1];
    let look = show.cue_look(demesure).unwrap();
    assert_eq!(look.gates.max_floor, 6);
    assert!(look.gates.salvo);
    assert_eq!(look.palette, PaletteRef::Dj);
    assert!((look.gates.cruise - 1.12).abs() < 1e-6);
    assert!((look.gates.fx.flash - 0.6).abs() < 1e-6);
    // Les champs non surchargés gardent la valeur du look.
    assert!((look.gates.fx.rgb_split - 0.3).abs() < 1e-6);
    assert_eq!(look.gates.cut_grid.groove, 4);
}

#[test]
fn event_looks_shadow_library() {
    let library = library();
    let text = std::fs::read_to_string(assets().join("shows/demo.ron")).unwrap();
    // L'événement embarque un look « ville-qui-monte » : il prime sur celui de la bibliothèque.
    let text = text
        .replacen("id: \"calme\"", "id: \"ville-qui-monte\"", 1)
        .replace("\"calme\"", "\"ville-qui-monte\"");
    let show = LoadedShow::from_ron(&text, "demo modifiée", &library).unwrap_or_else(|e| panic!("{e}"));
    let ville = show.look(&LookId::new("ville-qui-monte").unwrap()).unwrap();
    assert_eq!(ville.look.name, "Calme");
    assert!(matches!(ville.origin, LookOrigin::Event { shadows: Some(_) }));
}

#[test]
fn shipped_files_rewrite_identically() {
    let text = std::fs::read_to_string(assets().join("shows/demo.ron")).unwrap();
    let event = parse_event(&text, "demo").unwrap();
    let written = event_to_ron(&event).unwrap();
    assert_eq!(parse_event(&written, "réécrit").unwrap(), event);
    assert!(written.contains("version: 1"), "la version est écrite :\n{written}");
    let text = std::fs::read_to_string(assets().join("looks/ville-qui-monte.ron")).unwrap();
    let look = parse_look(&text, "look").unwrap();
    assert_eq!(parse_look(&look_to_ron(&look).unwrap(), "réécrit").unwrap(), look);
}
