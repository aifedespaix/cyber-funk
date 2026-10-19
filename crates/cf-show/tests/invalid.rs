//! Un fichier invalide donne une erreur lisible qui nomme le champ fautif (et la ligne pour une erreur de lecture).
//!
//! Chaque cas part de `assets/shows/demo.ron` (valide) et y introduit une seule faute.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::path::{Path, PathBuf};

use cf_show::load::{LoadError, LoadedShow, LookLibrary};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn library() -> LookLibrary {
    LookLibrary::load_dir(&assets().join("looks")).unwrap_or_else(|e| panic!("{e}"))
}

fn demo_text() -> String {
    std::fs::read_to_string(assets().join("shows/demo.ron")).unwrap()
}

/// Remplace exactement une occurrence de `from` dans la démo et charge le résultat ; renvoie l'erreur.
fn load_with(from: &str, to: &str) -> LoadError {
    let text = demo_text();
    assert_eq!(
        text.matches(from).count(),
        1,
        "« {from} » doit apparaître une seule fois dans la démo"
    );
    match LoadedShow::from_ron(&text.replacen(from, to, 1), "demo.ron", &library()) {
        Ok(_) => panic!("« {to} » aurait dû être refusé"),
        Err(error) => error,
    }
}

fn assert_field(error: &LoadError, field: &str, words: &[&str]) {
    let message = error.to_string();
    // Visible avec `--nocapture` : de quoi relire les messages tels que l'opérateur les verra.
    println!("{message}");
    assert_eq!(
        error.field(),
        Some(field),
        "champ fautif attendu : {field}\nmessage : {message}"
    );
    assert!(
        message.contains(field),
        "le message doit nommer le champ « {field} » :\n{message}"
    );
    for word in words {
        assert!(
            message.contains(word),
            "le message doit contenir « {word} » :\n{message}"
        );
    }
}

#[test]
fn out_of_range_number_names_its_field() {
    let error = load_with("cruise: 1.0,", "cruise: 3.5,");
    assert!(matches!(error, LoadError::Invalid { .. }), "{error}");
    assert_field(&error, "looks[0].gates.cruise", &["3.5", "[0.5, 2]"]);
}

#[test]
fn unknown_look_reference_names_its_field() {
    let error = load_with(
        "look: \"calme\",\n                    entry: Fade(bars: 8)",
        "look: \"cosmos\",\n                    entry: Fade(bars: 8)",
    );
    assert_field(&error, "slots[1].cues[2].look", &["cosmos", "introuvable"]);
}

#[test]
fn unknown_macro_path_names_its_field_and_line() {
    let text = demo_text().replacen(
        "            ),\n        ),\n    ],\n    slots",
        "            ),\n            macros: [Macro(name: \"X\", drives: [(\"gates.speed\", 0.0, 1.0)])],\n        ),\n    ],\n    slots",
        1,
    );
    let error = LoadedShow::from_ron(&text, "demo.ron", &library()).unwrap_err();
    let LoadError::Syntax { line, .. } = &error else {
        panic!("erreur de lecture attendue : {error}")
    };
    assert!(*line > 1);
    assert_field(
        &error,
        "looks[0].macros[0].drives[0][0]",
        &["gates.speed", "gates.cruise", &format!("demo.ron:{line}:")],
    );
}

#[test]
fn misspelled_field_is_named() {
    let error = load_with("advance: Bars(bars: 64)", "advnce: Bars(bars: 64)");
    let message = error.to_string();
    assert!(matches!(error, LoadError::Syntax { .. }), "{message}");
    assert!(message.contains("advnce"), "{message}");
    assert!(
        error.field().is_some_and(|f| f.starts_with("slots[1].cues[0]")),
        "{message}"
    );
}

#[test]
fn malformed_id_is_named() {
    let error = load_with("safe_look: \"calme\"", "safe_look: \"Calme Plat\"");
    assert_field(&error, "settings.safe_look", &["Calme Plat", "slug"]);
}

#[test]
fn several_problems_are_all_reported() {
    let text = demo_text()
        .replacen("bpm_min: 118.0, bpm_max: 128.0", "bpm_min: 128.0, bpm_max: 118.0", 1)
        .replacen("id: \"fin\"", "id: \"demesure\"", 1)
        .replacen("Fade(bars: 4)", "Fade(bars: 0)", 1);
    let error = LoadedShow::from_ron(&text, "demo.ron", &library()).unwrap_err();
    let LoadError::Invalid { issues, .. } = &error else {
        panic!("erreur de validation attendue : {error}")
    };
    let paths: Vec<&str> = issues.iter().map(|issue| issue.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "slots[0].audio.bpm_max",
            "slots[0].cues[1].entry.bars",
            "slots[1].cues[2]"
        ],
        "{error}"
    );
}

#[test]
fn versions_are_checked() {
    let missing = load_with("    version: 1,\n    settings", "    settings");
    assert!(matches!(missing, LoadError::MissingVersion { .. }), "{missing}");
    let newer = load_with("    version: 1,\n    settings", "    version: 7,\n    settings");
    assert!(matches!(newer, LoadError::TooNew { found: 7, .. }), "{newer}");
    assert!(newer.to_string().contains("mettre l'application à jour"));
    let zero = load_with("    version: 1,\n    settings", "    version: 0,\n    settings");
    assert!(matches!(zero, LoadError::UnknownVersion { found: 0, .. }), "{zero}");
}

#[test]
fn wrong_file_kind_is_reported() {
    let look = std::fs::read_to_string(assets().join("looks/ville-qui-monte.ron")).unwrap();
    let error = LoadedShow::from_ron(&look, "ville-qui-monte.ron", &library()).unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, LoadError::Syntax { .. }), "{message}");
    assert!(message.contains("Event") && message.contains("Look"), "{message}");
}

#[test]
fn library_look_must_match_file_name() {
    let text = std::fs::read_to_string(assets().join("looks/ville-qui-monte.ron")).unwrap();
    let mut library = LookLibrary::new();
    let error = library
        .insert_file(Path::new("assets/looks/autre-nom.ron"), &text)
        .unwrap_err();
    assert!(matches!(error, LoadError::LookIdMismatch { .. }), "{error}");
    assert!(error.to_string().contains("autre-nom"));
}
