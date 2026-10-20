//! Lecteur complet (décodeur + rendu) sur sortie manuelle, sans carte son, avec des WAV synthétiques dont chaque
//! échantillon est connu : on vérifie le signal à l'échantillon près.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use cf_audio::feed::{FeedReader, HOP, HopStatus};
use cf_audio::playlist::PlayMode;
use cf_audio::source::SourceKind;
use common::{RATE, approx, manual_player, pull_left, temp_dir, write_wav};

/// Durée des fondus du rendu en trames à 48 kHz (5 ms).
const FADE: usize = 240;

#[test]
fn sequence_is_gapless_and_sample_exact() {
    let dir = temp_dir("gapless");
    let (a, b) = (dir.join("A - Un.wav"), dir.join("B - Deux.wav"));
    write_wav(&a, RATE, 12_000, |_| 0.25);
    write_wav(&b, RATE, 9_000, |_| 0.5);
    let (player, mut driver) = manual_player(&[a, b], PlayMode::Sequence, false);
    assert!(driver.wait_buffered(20_000, Duration::from_secs(5)));
    player.play();

    let left = pull_left(&mut driver, 30_000, 256);
    // Fondu d'entrée au démarrage, puis A, puis B, puis A de nouveau (la séquence boucle), sans un seul échantillon
    // de silence ni de fondu entre les pistes.
    assert!(
        approx(left[0], 0.25 / FADE as f32),
        "rampe dès la première trame : {}",
        left[0]
    );
    assert!(left[FADE..12_000].iter().all(|&s| approx(s, 0.25)), "A entière");
    assert!(
        left[12_000..21_000].iter().all(|&s| approx(s, 0.5)),
        "B entière, collée à A"
    );
    assert!(
        left[21_000..].iter().all(|&s| approx(s, 0.25)),
        "A de nouveau, collée à B"
    );

    let state = player.state();
    assert_eq!(state.track, Some(0));
    assert_eq!(state.position_frames, 30_000 - 21_000, "position exacte à la trame");
    assert_eq!(state.rendered_frames, 30_000);
    assert_eq!(state.underruns, 0);
    assert_eq!(state.duration_s, Some(0.25));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn loop_mode_replays_the_track_without_gap() {
    let dir = temp_dir("loop");
    let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
    write_wav(&a, RATE, 6_000, |_| 0.25);
    write_wav(&b, RATE, 6_000, |_| 0.5);
    let (player, mut driver) = manual_player(&[a, b], PlayMode::Loop, false);
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));
    player.play();

    let left = pull_left(&mut driver, 20_000, 512);
    assert!(
        left[FADE..].iter().all(|&s| approx(s, 0.25)),
        "toujours A, jamais B ni silence"
    );
    let state = player.state();
    assert_eq!(state.track, Some(0));
    assert_eq!(state.position_frames, 20_000 % 6_000);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn volume_and_mute_never_touch_the_analysis_feed() {
    let dir = temp_dir("feed");
    let a = dir.join("a.wav");
    write_wav(&a, RATE, 48_000, |_| 0.25);
    let (mut player, mut driver) = manual_player(&[a], PlayMode::Loop, true);
    let input = player.take_analysis_feed().unwrap();
    assert!(player.take_analysis_feed().is_none(), "un seul flux d'analyse");
    assert_eq!(input.kind(), SourceKind::Demo);
    assert_eq!(input.sample_rate(), RATE);
    let mut reader = FeedReader::new(input).unwrap();
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));
    player.play();

    let left = pull_left(&mut driver, 4_096, 512);
    assert!(left.iter().all(|&s| s == 0.0), "sourdine : silence en sortie");
    let analysed = read_hops(&mut reader);
    assert_eq!(analysed.len(), 4_096, "un échantillon d'analyse par trame jouée");
    assert!(
        analysed[FADE..].iter().all(|&s| approx(s, 0.25)),
        "l'analyse entend la musique"
    );

    // Volume à moitié (gain 0,25 après la courbe quadratique), sourdine levée : la sortie suit le lissage, l'analyse
    // ne bouge pas.
    player.set_muted(false);
    player.set_volume(0.5);
    let left = pull_left(&mut driver, 48_000 / 2, 512);
    let last = *left.last().unwrap();
    assert!(approx(last, 0.25 * 0.25), "gain 0,25 atteint : {last}");
    let analysed = read_hops(&mut reader);
    // Pas complets seulement : la fin du dernier pas reste dans le lecteur du flux.
    assert_eq!(analysed.len(), (4_096 + 24_000) / HOP * HOP - 4_096);
    assert!(analysed.iter().all(|&s| approx(s, 0.25)));
    assert_eq!(player.state().analysis_dropped, 0);

    // En pause, rien ne part vers l'analyse (au-delà du fondu sortant) : c'est ce qui lui dit que la source se tait.
    player.pause();
    pull_left(&mut driver, 4_096, 512);
    let written = reader.stats().written();
    pull_left(&mut driver, 4_096, 512);
    assert_eq!(reader.stats().written(), written, "rien de poussé en pause");
    let after_pause = written - (4_096 + 24_000);
    // Le fondu compte 240 pas, plus la trame où il touche zéro (arrondi f32 compris).
    assert!(
        after_pause <= FADE as u64 + 1,
        "seul le fondu sortant est parti : {after_pause}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Lit tous les pas d'analyse complets disponibles.
fn read_hops(reader: &mut FeedReader) -> Vec<f32> {
    let mut out = Vec::new();
    let mut hop = [0.0_f32; HOP];
    while let Ok(HopStatus::Ready { .. }) = reader.next_hop(&mut hop, Instant::now()) {
        out.extend_from_slice(&hop);
    }
    out
}

#[test]
fn pause_fades_out_freezes_position_and_resumes() {
    let dir = temp_dir("pause");
    let a = dir.join("a.wav");
    write_wav(&a, RATE, 96_000, |_| 0.25);
    let (player, mut driver) = manual_player(&[a], PlayMode::Loop, false);
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));
    player.play();
    pull_left(&mut driver, 1_000, 500);

    player.pause();
    let left = pull_left(&mut driver, 2_000, 500);
    let silent_from = left.iter().position(|&s| s == 0.0).unwrap();
    assert!(silent_from <= FADE, "fondu sortant de 5 ms au plus : {silent_from}");
    assert!(
        left[..silent_from].windows(2).all(|w| w[1] <= w[0]),
        "décroissance monotone, sans claquement"
    );
    assert!(left[silent_from..].iter().all(|&s| s == 0.0));
    let paused_at = player.state().position_frames;
    pull_left(&mut driver, 2_000, 500);
    assert_eq!(player.state().position_frames, paused_at, "position figée en pause");
    assert!(!player.state().playing);

    player.toggle();
    let left = pull_left(&mut driver, 1_000, 500);
    assert!(
        left[0] < 0.01 && approx(left[FADE + 10], 0.25),
        "fondu d'entrée à la reprise"
    );
    assert_eq!(player.state().position_frames, paused_at + 1_000);
    assert_eq!(player.state().underruns, 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Rend par petits blocs jusqu'à ce que `done` soit vrai (le décodeur traite les commandes à son rythme). Rend le
/// signal rendu entre-temps, ou `None` si la condition n'arrive jamais.
fn pull_until(driver: &mut cf_audio::player::ManualDriver, mut done: impl FnMut() -> bool) -> Option<Vec<f32>> {
    let mut left = Vec::new();
    for _ in 0..2_000 {
        if done() {
            return Some(left);
        }
        left.extend(pull_left(driver, 128, 128));
        sleep(Duration::from_millis(1));
    }
    None
}

#[test]
fn next_prev_and_select_jump_with_short_fades() {
    let dir = temp_dir("jump");
    let paths: Vec<_> = (0..3).map(|i| dir.join(format!("t{i}.wav"))).collect();
    for (i, path) in paths.iter().enumerate() {
        write_wav(path, RATE, 4 * RATE as usize, move |_| 0.1 * (i + 1) as f32);
    }
    let (player, mut driver) = manual_player(&paths, PlayMode::Sequence, false);
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));
    player.play();
    pull_left(&mut driver, 2_000, 500);

    player.next();
    let jump = pull_until(&mut driver, || {
        let s = player.state();
        // Position > 0 : t1 a commencé à jouer (après le court silence d'amorçage qui suit un saut).
        s.track == Some(1) && s.position_frames > 0
    })
    .expect("passage à t1");
    // Fondu sortant de t0 puis fondu entrant de t1 : aucune marche d'un échantillon à l'autre (pas de claquement).
    let steepest = jump.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0_f32, f32::max);
    assert!(
        steepest < 0.001,
        "variation maximale entre deux échantillons : {steepest}"
    );
    let after = pull_left(&mut driver, 1_000, 500);
    assert!(approx(after[999], 0.2), "t1 joue : {}", after[999]);

    // Moins de 3 s écoutées : « précédent » revient à t0 ; au-delà, il relance t1.
    player.prev();
    pull_until(&mut driver, || player.state().track == Some(0)).expect("retour à t0");
    player.select(2);
    pull_until(&mut driver, || player.state().track == Some(2)).expect("t2 choisi");
    pull_left(&mut driver, 3 * RATE as usize + RATE as usize / 2, 4_096);
    player.prev();
    pull_until(&mut driver, || player.state().position_frames < u64::from(RATE)).expect("t2 relancé");
    assert_eq!(player.state().track, Some(2), "relancé, pas changé");
    assert_eq!(player.state().underruns, 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn seek_lands_on_the_requested_frame() {
    let dir = temp_dir("seek");
    let a = dir.join("ramp.wav");
    // Rampe de 0 à 0,9 sur 2 s : la valeur entendue dit où l'on est dans la piste.
    let frames = 2 * RATE as usize;
    write_wav(&a, RATE, frames, move |i| 0.9 * i as f32 / frames as f32);
    let (player, mut driver) = manual_player(&[a], PlayMode::Loop, false);
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));
    player.play();
    pull_left(&mut driver, 1_000, 500);

    player.seek(1.5);
    pull_until(&mut driver, || player.state().position_frames > 72_000).expect("seek appliqué");
    let before = player.state().position_frames;
    let left = pull_left(&mut driver, 1_000, 500);
    let expected = 0.9 * (before + 999) as f32 / frames as f32;
    assert!(
        (left[999] - expected).abs() < 1e-3,
        "signal à la position annoncée : {} vs {expected}",
        left[999]
    );
    assert!(before - 72_000 < 4_000, "position juste après la cible : {before}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tracks_at_another_rate_are_resampled_to_exact_length() {
    let dir = temp_dir("resample");
    let (slow, b) = (dir.join("a44.wav"), dir.join("b.wav"));
    write_wav(&slow, 44_100, 44_100, |_| 0.3);
    write_wav(&b, RATE, 12_000, |_| 0.5);
    let (player, mut driver) = manual_player(&[slow, b], PlayMode::Sequence, false);
    assert!(driver.wait_buffered(20_000, Duration::from_secs(5)));
    player.play();

    let left = pull_left(&mut driver, 60_000, 1_024);
    let b_starts = left.iter().position(|&s| approx(s, 0.5)).unwrap();
    assert_eq!(b_starts, 48_000, "1 s à 44,1 kHz = 48 000 trames à 48 kHz, sans trou");
    assert!(
        left[2_000..46_000].iter().all(|&s| (s - 0.3).abs() < 1e-3),
        "niveau conservé"
    );
    assert_eq!(player.state().duration_s, Some(0.25));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unreadable_tracks_are_skipped_and_empty_playlist_stays_silent() {
    let dir = temp_dir("broken");
    let (bad, good) = (dir.join("bad.mp3"), dir.join("good.wav"));
    std::fs::write(&bad, b"pas du mp3").unwrap();
    write_wav(&good, RATE, 12_000, |_| 0.25);
    let (player, mut driver) = manual_player(&[bad.clone(), good], PlayMode::Sequence, false);
    assert!(driver.wait_buffered(10_000, Duration::from_secs(5)));
    player.play();
    let left = pull_left(&mut driver, 1_000, 500);
    assert!(approx(left[999], 0.25));
    assert_eq!(player.state().track, Some(1));

    // Rien de lisible : silence, et un ring vide n'est pas une sous-alimentation.
    let (player, mut driver) = manual_player(&[bad], PlayMode::Sequence, false);
    player.play();
    let mut buf = vec![1.0_f32; 1_024];
    for _ in 0..50 {
        sleep(Duration::from_millis(1));
        assert!(driver.pull(&mut buf, 2));
    }
    assert!(buf.iter().all(|&s| s == 0.0));
    assert_eq!(player.state().track, None);
    assert_eq!(player.state().underruns, 0);
    std::fs::remove_dir_all(&dir).unwrap();
}
