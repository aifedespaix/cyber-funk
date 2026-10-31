//! J9-02 : un look corrompu n'empêche pas d'ouvrir le show, les cues concernés jouent le look calme.

#![cfg(feature = "fault-injection")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::fs;
use std::path::PathBuf;

use cf_show::fault::{Corruption, corrupt};
use cf_show::load::{LoadedShow, LookLibrary, parse_event};
use cf_show::model::LookId;

const EVENT: &str = r#"Event(
    version: 1,
    settings: (
        name: "Panne de look",
        theme: Preset(Velvet),
        flashes: Limited,
        latency_offset_ms: 0.0,
        projector: Auto,
        safe_look: "niveau-0-pre-intro",
        dj_sign: true,
        dj_on_ads: true,
    ),
    slots: [DjSlot(
        id: "a",
        dj: (name: "A"),
        audio: (bpm_min: 100.0, bpm_max: 140.0, sensitivity: 0.5, tempo_source: Auto),
        mode: Semi,
        cues: [
            Cue(id: "c1", name: "Ville", look: "niveau-1-ville", entry: Cut, advance: Manual),
            Cue(id: "c2", name: "Rues", look: "niveau-2-rues", entry: Cut, advance: Manual),
        ],
    )],
)"#;

fn assets_looks() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/looks")
}

/// Copie des looks livrés dans un dossier temporaire, dont `victim` est abîmé.
fn copy_with_corruption(victim: &str, how: Corruption) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in fs::read_dir(assets_looks()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_owned();
        let mut text = fs::read_to_string(&path).unwrap();
        if path.file_stem().unwrap() == victim {
            text = corrupt(&text, how);
        }
        fs::write(dir.path().join(name), text).unwrap();
    }
    dir
}

#[test]
fn every_kind_of_corruption_falls_back_to_the_safe_look_and_the_show_opens() {
    for how in [Corruption::Empty, Corruption::Truncated, Corruption::Garbled] {
        let dir = copy_with_corruption("niveau-2-rues", how);

        // La version stricte refuse (on le sait avant le show), la tolérante écarte le fichier.
        assert!(LookLibrary::load_dir(dir.path()).is_err(), "{how:?}");
        let (library, rejected) = LookLibrary::load_dir_lenient(dir.path()).unwrap();
        assert_eq!(rejected.len(), 1, "{how:?} : un seul fichier écarté");
        assert!(!library.contains(&LookId::new("niveau-2-rues").unwrap()));

        let event = parse_event(EVENT, "panne").unwrap();
        let (show, substitutions) = LoadedShow::new_resilient(event, &library, "panne").unwrap();
        assert_eq!(substitutions.len(), 1, "{how:?}");
        assert_eq!((substitutions[0].slot, substitutions[0].cue), (0, 1));
        assert_eq!(substitutions[0].wanted.as_str(), "niveau-2-rues");

        let cues = &show.event().slots[0].cues;
        assert_eq!(cues[0].look.as_str(), "niveau-1-ville", "le cue sain est intact");
        assert_eq!(
            cues[1].look.as_str(),
            "niveau-0-pre-intro",
            "le cue abîmé joue le look calme"
        );
        assert!(show.cue_look(&cues[1]).is_some());
    }
}

#[test]
fn a_missing_safe_look_has_no_fallback_and_is_refused() {
    let dir = copy_with_corruption("niveau-0-pre-intro", Corruption::Truncated);
    let (library, _) = LookLibrary::load_dir_lenient(dir.path()).unwrap();
    let event = parse_event(EVENT, "panne").unwrap();
    assert!(LoadedShow::new_resilient(event, &library, "panne").is_err());
}
