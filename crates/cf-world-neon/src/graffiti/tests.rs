//! Tests des données de graffitis : chargeurs tolérants et fichiers livrés.

use super::*;
use std::path::PathBuf;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

#[test]
fn une_entree_cassee_est_ignoree_et_les_autres_restent() {
    let texte = r#"[
        (text: "A", font: "F", style: Fill, wall: true, holo: false, hues: [10.0]),
        (text: "B", font: "F", style: Inconnu, wall: true, holo: false, hues: []),
        (text: "C", font: "F", style: Outline, wall: false, holo: true, hues: []),
    ]"#;
    let (mots, avertissements) = load_words(texte);
    assert_eq!(mots.len(), 2);
    assert_eq!(mots[1].text, "C");
    assert_eq!(avertissements.len(), 1);

    let placements = r#"[
        (z_m: 1.0, side: 1, height_m: 2.0, size: Small, word: "A", yaw_deg: 0.0),
        (z_m: "oups"),
        (z_m: 5.0, side: -1, height_m: 2.0, size: Large, word: "C", yaw_deg: 3.0),
    ]"#;
    let (liste, avertissements) = load_placements(placements);
    assert_eq!(liste.len(), 2);
    assert_eq!(liste[1].size, ProjectorSize::Large);
    assert_eq!(avertissements.len(), 1);
}

#[test]
fn un_fichier_vide_donne_zero_mot_sans_erreur() {
    for texte in ["", "   \n", "[]"] {
        let (mots, avertissements) = load_words(texte);
        assert!(mots.is_empty() && avertissements.is_empty(), "{texte:?}");
        let (liste, avertissements) = load_placements(texte);
        assert!(liste.is_empty() && avertissements.is_empty(), "{texte:?}");
    }
}

#[test]
fn un_fichier_illisible_donne_un_avertissement_sans_panique() {
    let (mots, avertissements) = load_words("(pas une liste");
    assert_eq!(mots.len(), 0);
    assert_eq!(avertissements.len(), 1);
}

#[test]
fn les_tailles_ont_les_dimensions_de_la_spec() {
    assert_eq!(ProjectorSize::Small.bar_len_m(), 2.0);
    assert_eq!(ProjectorSize::Small.letter_h_m(), 1.0);
    assert_eq!(ProjectorSize::Medium.bar_len_m(), 6.0);
    assert_eq!(ProjectorSize::Medium.letter_h_m(), 3.0);
    assert_eq!(ProjectorSize::Large.bar_len_m(), 20.0);
    assert_eq!(ProjectorSize::Large.letter_h_m(), 10.0);
}

#[test]
fn licensed_fonts_ignore_les_lignes_a_completer() {
    let md = "| Nom | Fichier | Source | Licence | Date |\n| --- | --- | --- | --- | --- |\n\
              | Un | `un.ttf` | s | CC0 d'après l'utilisateur | 2026-11-03 |\n\
              | Deux | `deux.otf` | inconnue | À COMPLÉTER | |\n";
    assert_eq!(licensed_fonts(md), vec!["un".to_string()]);
}

#[test]
fn chaque_police_versionnee_a_une_ligne_de_licence() {
    let dossier = repo_file("assets/fonts");
    let mut versionnees: Vec<String> = std::fs::read_dir(&dossier)
        .expect("assets/fonts lisible")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("ttf" | "otf")))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    versionnees.sort();
    let md = std::fs::read_to_string(dossier.join("LICENCES.md")).expect("LICENCES.md lisible");
    let mut licenciees = licensed_fonts(&md);
    licenciees.sort();
    assert_eq!(versionnees, licenciees);
}

#[test]
fn le_words_ron_livre_se_charge_sans_avertissement() {
    let texte = std::fs::read_to_string(repo_file("assets/graffiti/words.ron")).expect("words.ron");
    let (mots, avertissements) = load_words(&texte);
    assert!(avertissements.is_empty(), "{avertissements:?}");
    assert!((20..=30).contains(&mots.len()), "{} mots", mots.len());
    assert!(mots.iter().any(|m| m.wall && !m.holo) && mots.iter().any(|m| !m.wall && m.holo));
    let md = std::fs::read_to_string(repo_file("assets/fonts/LICENCES.md")).expect("LICENCES.md");
    let polices = licensed_fonts(&md);
    for mot in &mots {
        assert!(polices.contains(&mot.font), "police sans licence : {}", mot.font);
    }
}

#[test]
fn le_placements_ron_livre_se_charge_sans_avertissement() {
    let texte = std::fs::read_to_string(repo_file("assets/graffiti/placements.ron")).expect("placements.ron");
    let (liste, avertissements) = load_placements(&texte);
    assert!(avertissements.is_empty(), "{avertissements:?}");
    for taille in [ProjectorSize::Small, ProjectorSize::Medium, ProjectorSize::Large] {
        assert!(liste.iter().any(|p| p.size == taille), "{taille:?} absent");
    }
    let mots = std::fs::read_to_string(repo_file("assets/graffiti/words.ron")).expect("words.ron");
    let (mots, _) = load_words(&mots);
    for p in &liste {
        assert!(
            mots.iter().any(|m| m.text == p.word && m.holo),
            "mot inconnu ou non holo : {}",
            p.word
        );
    }
}
