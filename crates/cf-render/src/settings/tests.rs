//! Le plugin d'écriture automatique, dans une `App` minimale avec une horloge réelle simulée.

#![allow(
    clippy::unwrap_used,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_show::persist::{AppSettings, CarChoice};

use super::{Settings, SettingsPlugin};

fn app(path: std::path::PathBuf) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
        .add_plugins(SettingsPlugin {
            path,
            initial: AppSettings::default(),
        });
    app
}

fn run(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 10.0) as usize {
        app.update();
        // Laisse le pool d'E/S finir l'écriture.
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn un_changement_est_ecrit_apres_le_calme() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    let mut app = app(path.clone());
    run(&mut app, 0.5);
    app.world_mut().resource_mut::<Settings>().value.car = CarChoice::Cyber;
    run(&mut app, 0.5);
    assert!(!path.exists(), "pas d'écriture avant la seconde de calme");
    run(&mut app, 2.0);
    assert_eq!(AppSettings::load(&path).car, CarChoice::Cyber);
}

#[test]
fn sans_changement_aucun_fichier_n_est_cree() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    let mut app = app(path.clone());
    // `Settings` vient d'être inséré : le premier passage le voit « changé » une fois, puis plus rien.
    run(&mut app, 5.0);
    let first = path.exists();
    if first {
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        run(&mut app, 3.0);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            written,
            "pas de réécriture"
        );
    }
}
