//! Tests de la résolution du binaire et du format de date.

use super::*;

#[test]
fn format_utc_connait_l_epoque_et_une_date_recente() {
    assert_eq!(format_utc(UNIX_EPOCH), "1970-01-01 00:00:00");
    assert_eq!(
        format_utc(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        "2023-11-14 22:13:20"
    );
}

#[test]
fn sans_binaire_compile_l_erreur_donne_la_commande_de_compilation() {
    let target = tempfile::tempdir().unwrap();
    let error = build_command_in(target.path(), target.path(), &LaunchOptions::default()).unwrap_err();
    assert!(
        format!("{error:#}").contains("cargo build -p cyber-funk --features probe"),
        "{error:#}"
    );
}

#[test]
fn avec_un_binaire_compile_on_rend_son_chemin_et_sa_date() {
    let target = tempfile::tempdir().unwrap();
    let debug = target.path().join("debug");
    std::fs::create_dir_all(&debug).unwrap();
    let name = format!("cyber-funk{}", std::env::consts::EXE_SUFFIX);
    std::fs::write(debug.join(&name), b"").unwrap();
    let resolved = build_command_in(target.path(), target.path(), &LaunchOptions::default()).unwrap();
    let note = resolved.describe();
    assert!(note.contains(&name) && note.contains("UTC"), "{note}");
}
