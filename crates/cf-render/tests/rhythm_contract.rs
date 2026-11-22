//! Cliquet du contrat rythmique (`docs/rythme.md`, lot R1).
//!
//! Les effets lisent le rythme par `FilteredSignals` (phase, onde de kick, `grid_trust`), pas dans `AudioFeatures`. Le
//! test balaie les sources de `cf-world-neon`, `cf-cinema` et `cf-render` (hors `audio/`, hors tests et exemples) et
//! repère les fichiers qui nomment `AudioFeatures` **et** lisent un de ses champs de grille ou d'accent (`kick`,
//! `pulse`, `count`, `phase`, `bar`, `phrase`, `beat`, `onset`, `hat`, `transient`). Il échoue :
//!
//! - pour un fichier hors de `rhythm_allowlist.txt` qui lit un de ces champs : nouvelle lecture directe ;
//! - pour une entrée de la liste qui n'en lit plus : la liste ne fait que rétrécir.
//!
//! Une entrée peut rester si une raison est écrite sur la ligne (`chemin # raison`). L'énergie (`bass`, `intensity`,
//! `section`, `live`, `bpm`…) peut rester lue dans `AudioFeatures` : ce n'est ni de la grille ni un accent.
//!
//! Heuristique de texte : un champ `.phase` d'un autre type dans un fichier qui nomme `AudioFeatures` compte aussi ; la
//! raison s'écrit alors dans la liste.

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const FIELDS: [&str; 10] = [
    "kick",
    "pulse",
    "count",
    "phase",
    "bar",
    "phrase",
    "beat",
    "onset",
    "hat",
    "transient",
];

/// Vrai si le code (sans commentaires) lit l'un des champs : `.champ` non suivi d'un identifiant ni d'une parenthèse.
fn reads_rhythm_field(code: &str) -> bool {
    let bytes = code.as_bytes();
    for field in FIELDS {
        let pat = format!(".{field}");
        let mut from = 0;
        while let Some(pos) = code[from..].find(&pat) {
            let end = from + pos + pat.len();
            let next = bytes.get(end).copied();
            let continues = next.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'(');
            if !continues {
                return true;
            }
            from = end;
        }
    }
    false
}

/// Le code d'un fichier : commentaires retirés, coupé avant le premier module de tests.
fn production_code(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let code = line.split("//").next().unwrap_or("");
        out.push_str(code);
        out.push('\n');
    }
    out
}

/// Vrai si le fichier nomme `AudioFeatures` (le type, éventuellement sous alias) et lit un champ de rythme.
fn violates(source: &str) -> bool {
    let code = production_code(source);
    code.contains("AudioFeatures") && reads_rhythm_field(&code)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Chemin relatif à `crates/`, avec des `/`.
fn relative(path: &Path, crates: &Path) -> String {
    path.strip_prefix(crates)
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn excluded(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.contains("/examples/")
        || rel.contains("/tests/")
        || rel.starts_with("cf-render/src/audio/")
        || name.starts_with("tests")
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
}

fn allowlist(path: &Path) -> BTreeSet<String> {
    fs::read_to_string(path)
        .expect("rhythm_allowlist.txt")
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn rhythm_reads_only_through_the_contract() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = manifest.join("..");
    let mut files = Vec::new();
    for member in ["cf-world-neon", "cf-cinema", "cf-render"] {
        rust_files(&crates.join(member).join("src"), &mut files);
    }
    let found: BTreeSet<String> = files
        .iter()
        .map(|p| relative(p, &crates))
        .filter(|rel| !excluded(rel))
        .filter(|rel| violates(&fs::read_to_string(crates.join(rel)).expect("source lisible")))
        .collect();
    let allowed = allowlist(&manifest.join("tests/rhythm_allowlist.txt"));

    let new: Vec<_> = found.difference(&allowed).collect();
    let stale: Vec<_> = allowed.difference(&found).collect();
    assert!(
        new.is_empty(),
        "lecture directe d'AudioFeatures (grille ou accent) : passer par FilteredSignals (docs/rythme.md). \
         Fichiers :\n{}",
        new.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    );
    assert!(
        stale.is_empty(),
        "entrées de rhythm_allowlist.txt qui ne lisent plus rien : les retirer (la liste ne fait que rétrécir) :\n{}",
        stale.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_scanner_sees_fields_but_not_iterators_or_comments() {
    assert!(reads_rhythm_field("let k = features.kick;"));
    assert!(reads_rhythm_field("if f.pulse > 0.5 {}"));
    assert!(reads_rhythm_field("let n = f.count + 1;"));
    assert!(!reads_rhythm_field("let n = items.iter().count();"));
    assert!(!reads_rhythm_field("let p = x.phase_offset;"));
    assert!(!reads_rhythm_field("let b = x.beats;"));
    assert!(violates(
        "use cf_analysis::AudioFeatures;\nfn a(f: &AudioFeatures) { let _ = f.kick; }"
    ));
    assert!(!violates("fn a(f: &Other) { let _ = f.kick; }"));
    assert!(!violates(
        "use X::AudioFeatures;\n// f.kick\n#[cfg(test)]\nmod t { fn b(f:&X){ f.kick; } }"
    ));
}
