//! Garde-fou de la règle 6 de CLAUDE.md : pas de `NoFrustumCulling` sans commentaire qui explique pourquoi et quel
//! `Aabb` aurait été impossible.
//!
//! Le test parcourt les sources de tous les crates du dépôt (`src`, `examples`, `tests`, `benches`) et refuse toute
//! ligne de code qui nomme le composant sans, dans les six lignes qui la précèdent, un commentaire `//` qui parle
//! d'`Aabb`. Il ne juge pas la justification, seulement qu'elle existe : le relecteur juge.
//!
//! Les lignes de commentaire ne comptent pas comme usage (la documentation peut nommer le composant pour l'interdire).

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &str = "NoFrustumCulling";
/// Lignes examinées au-dessus de l'usage.
const WINDOW: usize = 6;

/// Numéros (à partir de 1) des lignes qui nomment le composant sans justification au-dessus.
fn violations(source: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || !line.contains(FORBIDDEN) {
            continue;
        }
        let from = i.saturating_sub(WINDOW);
        let justified = lines[from..i].iter().any(|above| {
            let above = above.trim_start();
            above.starts_with("//") && above.contains("Aabb")
        });
        if !justified {
            found.push(i + 1);
        }
    }
    found
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

#[test]
fn aucun_no_frustum_culling_sans_justification() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let this = Path::new(file!()).file_name().map(ToOwned::to_owned);
    let mut files = Vec::new();
    for member in fs::read_dir(&crates).expect("dossier crates").flatten() {
        for sub in ["src", "examples", "tests", "benches"] {
            rust_files(&member.path().join(sub), &mut files);
        }
    }
    assert!(
        files.len() > 20,
        "le parcours ne trouve presque aucun fichier : {}",
        files.len()
    );
    let mut bad = Vec::new();
    for file in files {
        // Ce fichier cite le composant dans ses exemples de test.
        if file.file_name().map(ToOwned::to_owned) == this {
            continue;
        }
        let source = fs::read_to_string(&file).expect("source lisible");
        for line in violations(&source) {
            bad.push(format!("{}:{line}", file.display()));
        }
    }
    assert!(
        bad.is_empty(),
        "`NoFrustumCulling` sans commentaire sur l'`Aabb` impossible (règle 6 de CLAUDE.md) :\n{}",
        bad.join("\n")
    );
}

#[test]
fn le_scanner_accepte_un_usage_justifie() {
    let source = "\
// Le mesh est déformé par un shader de vertex qui sort de tout Aabb borné : la houle
// n'a pas d'amplitude maximale connue.
commands.spawn((Mesh3d(m), NoFrustumCulling));
";
    assert_eq!(violations(source), Vec::<usize>::new());
}

#[test]
fn le_scanner_refuse_un_usage_nu_ou_un_commentaire_sans_aabb() {
    let bare = "commands.spawn((Mesh3d(m), NoFrustumCulling));\n";
    assert_eq!(violations(bare), vec![1]);
    let off_topic = "// pour que ça marche\ncommands.spawn((Mesh3d(m), NoFrustumCulling));\n";
    assert_eq!(violations(off_topic), vec![2]);
    // Un commentaire trop loin au-dessus ne compte pas.
    let far = format!(
        "// Aabb impossible\n{}x.insert(NoFrustumCulling);\n",
        "let a = 1;\n".repeat(WINDOW + 1)
    );
    assert_eq!(violations(&far), vec![WINDOW + 3]);
}

#[test]
fn le_scanner_ignore_la_documentation_qui_interdit_le_composant() {
    let source = "/// Jamais `NoFrustumCulling` ici.\n//! Voir NoFrustumCulling.\n// NoFrustumCulling : non.\n";
    assert_eq!(violations(source), Vec::<usize>::new());
}
