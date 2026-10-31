//! Réglages de l'application : fichier RON tolérant, écriture atomique.

#![allow(
    clippy::unwrap_used,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::fs;

use cf_show::persist::{AppSettings, CarChoice, SETTINGS_VERSION};

fn sample() -> AppSettings {
    AppSettings {
        audio_source: Some("Line".into()),
        mic_device: Some("Micro USB".into()),
        line_device: Some("Focusrite".into()),
        midi_inputs: vec!["APC40".into(), "Launchpad".into()],
        quality: Some("high".into()),
        car: CarChoice::Cyber,
        collapsed_panels: vec!["Trafic".into()],
        ..AppSettings::default()
    }
}

#[test]
fn un_fichier_absent_donne_les_defauts() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        AppSettings::load(&dir.path().join("settings.ron")),
        AppSettings::default()
    );
}

#[test]
fn un_fichier_tronque_donne_les_defauts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    fs::write(&path, "(version: 1, audio_source: Some(\"Li").unwrap();
    assert_eq!(AppSettings::load(&path), AppSettings::default());
}

#[test]
fn un_champ_inconnu_est_ignore_et_les_autres_sont_gardes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    fs::write(
        &path,
        "(champ_du_futur: 12, mic_device: Some(\"Micro USB\"), quality: 7)",
    )
    .unwrap();
    let loaded = AppSettings::load(&path);
    assert_eq!(loaded.mic_device.as_deref(), Some("Micro USB"));
    // Un champ de mauvais type ne fait pas perdre les autres.
    assert_eq!(loaded.quality, None);
}

#[test]
fn une_voiture_inconnue_donne_la_dirty_car() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    fs::write(&path, "(car: \"hovercraft\", mic_device: Some(\"X\"))").unwrap();
    let loaded = AppSettings::load(&path);
    assert_eq!(loaded.car, CarChoice::Dirty);
    assert_eq!(loaded.mic_device.as_deref(), Some("X"));
}

#[test]
fn un_aller_retour_conserve_tout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sous/dossier/settings.ron");
    let settings = sample();
    settings.save(&path).unwrap();
    assert_eq!(AppSettings::load(&path), settings);
    assert_eq!(settings.version, SETTINGS_VERSION);
}

#[test]
fn l_ecriture_laisse_l_ancien_fichier_intact_si_le_nouveau_est_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    sample().save(&path).unwrap();
    // Le chemin temporaire est occupé par un dossier : la création du fichier temporaire échoue.
    fs::create_dir(dir.path().join("settings.ron.tmp")).unwrap();
    let changed = AppSettings {
        car: CarChoice::Dirty,
        ..sample()
    };
    assert!(changed.save(&path).is_err());
    assert_eq!(AppSettings::load(&path), sample());
}
