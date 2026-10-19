//! Le rendu (ce qui tourne dans le callback de la carte son) n'alloue jamais : lecture, enchaînement de pistes,
//! pause, reprise, saut, seek, voie d'analyse pleine, volume et sourdine. `assert_no_alloc` remplace l'allocateur de
//! ce binaire de test et fait avorter le processus à la moindre allocation sur le thread qui rend.

mod common;

use std::time::Duration;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_audio::player::ManualDriver;
use cf_audio::playlist::PlayMode;
use common::{RATE, manual_player, temp_dir, write_wav};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

/// Rend `blocks` blocs de 256 trames sans aucune allocation. Le tampon et l'attente du décodeur sont hors zone.
#[allow(clippy::expect_used)] // aide de test : `allow-expect-in-tests` ne couvre que les fonctions `#[test]`
fn render_without_alloc(driver: &mut ManualDriver, buf: &mut [f32], blocks: usize) {
    for _ in 0..blocks {
        driver.wait_buffered(256, Duration::from_secs(5));
        let renderer = driver.renderer().expect("sortie démarrée");
        assert_no_alloc(|| renderer.render(buf, 2));
    }
}

#[test]
fn render_never_allocates() {
    let dir = temp_dir("noalloc");
    let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
    write_wav(&a, RATE, 6_000, |i| (i as f32 * 0.01).sin() * 0.5);
    write_wav(&b, 44_100, 30_000, |i| (i as f32 * 0.02).sin() * 0.5);
    let (mut player, mut driver) = manual_player(&[a, b], PlayMode::Sequence, false);
    // Flux d'analyse pris mais jamais lu : il déborde, le chemin « ring plein » est donc couvert.
    let _feed = player.take_analysis_feed().expect("flux d'analyse");
    let mut buf = vec![0.0_f32; 256 * 2];
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));

    render_without_alloc(&mut driver, &mut buf, 4); // en pause
    player.play();
    render_without_alloc(&mut driver, &mut buf, 400); // plusieurs enchaînements a → b → a
    player.set_volume(0.3);
    player.set_muted(true);
    render_without_alloc(&mut driver, &mut buf, 20);
    player.set_muted(false);
    player.pause();
    render_without_alloc(&mut driver, &mut buf, 20);
    player.play();
    render_without_alloc(&mut driver, &mut buf, 20);
    player.next();
    render_without_alloc(&mut driver, &mut buf, 40);
    player.seek(0.2);
    render_without_alloc(&mut driver, &mut buf, 40);

    // Sortie mono et 6 voies : même chemin, autre répartition.
    let mut mono = vec![0.0_f32; 256];
    let mut surround = vec![0.0_f32; 256 * 6];
    let renderer = driver.renderer().expect("sortie démarrée");
    assert_no_alloc(|| {
        renderer.render(&mut mono, 1);
        renderer.render(&mut surround, 6);
    });

    let state = player.state();
    assert!(
        state.analysis_dropped > 0,
        "le débordement du flux d'analyse a bien été exercé"
    );
    assert_eq!(state.underruns, 0);
    std::fs::remove_dir_all(&dir).unwrap();
}
