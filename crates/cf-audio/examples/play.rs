//! Lecture des pistes de démo.
//!
//! ```text
//! cargo run -p cf-audio --example play                      # carte son par défaut, mode hasard
//! cargo run -p cf-audio --example play -- --mode sequence --volume 0.5
//! cargo run -p cf-audio --example play -- --device "USB"    # sortie dont le nom contient « USB »
//! cargo run -p cf-audio --example play -- --list-devices
//! cargo run -p cf-audio --release --example play -- --null 40 --mode sequence --once
//! ```
//!
//! `--null <vitesse>` remplace la carte son par une sortie nulle cadencée (vitesse réelle × N) : de quoi parcourir
//! toutes les démos en quelques secondes sur une machine sans audio et lire le compteur de sous-alimentations.
//! `--once` s'arrête après un tour complet de la playlist (mode séquence). Le flux d'analyse est branché et lu comme
//! le ferait le thread d'analyse : le journal affiche son niveau, qui ne bouge pas avec `--volume 0` ou `--mute`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use cf_audio::feed::{FeedReader, HOP, HopStatus};
use cf_audio::player::{CpalOutput, NullOutput, OutputBackend, Player, PlayerConfig, PlayerState};
use cf_audio::playlist::{PlayMode, Playlist, scan_music_dir};

#[derive(Debug)]
struct Options {
    music_dir: PathBuf,
    device: Option<String>,
    null_speed: Option<f64>,
    mode: PlayMode,
    volume: f32,
    muted: bool,
    track: Option<usize>,
    once: bool,
    max_seconds: Option<f64>,
    list_devices: bool,
}

fn parse_options() -> anyhow::Result<Options> {
    let mut options = Options {
        music_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/music"),
        device: None,
        null_speed: None,
        mode: PlayMode::Random,
        volume: 0.8,
        muted: false,
        track: None,
        once: false,
        max_seconds: None,
        list_devices: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("valeur attendue après {arg}"));
        match arg.as_str() {
            "--dir" => options.music_dir = PathBuf::from(value()?),
            "--device" => options.device = Some(value()?),
            "--null" => options.null_speed = Some(value()?.parse().context("--null <vitesse>")?),
            "--volume" => options.volume = value()?.parse().context("--volume <0..1>")?,
            "--track" => options.track = Some(value()?.parse().context("--track <indice>")?),
            "--seconds" => options.max_seconds = Some(value()?.parse().context("--seconds <durée>")?),
            "--mode" => {
                options.mode = match value()?.as_str() {
                    "random" => PlayMode::Random,
                    "sequence" => PlayMode::Sequence,
                    "loop" => PlayMode::Loop,
                    other => bail!("mode inconnu : {other} (random, sequence, loop)"),
                }
            }
            "--mute" => options.muted = true,
            "--once" => options.once = true,
            "--list-devices" => options.list_devices = true,
            other => bail!("option inconnue : {other}"),
        }
    }
    Ok(options)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let options = parse_options()?;
    if options.list_devices {
        for name in CpalOutput::device_names()? {
            tracing::info!(%name, "périphérique de sortie");
        }
        return Ok(());
    }

    let tracks = scan_music_dir(&options.music_dir)?;
    if tracks.is_empty() {
        bail!("aucune piste dans {}", options.music_dir.display());
    }
    let count = tracks.len();
    for (i, track) in tracks.iter().enumerate() {
        tracing::info!(i, artist = %track.artist, title = %track.title, local = track.local, "piste");
    }
    let start = options.track.or(if options.once { Some(0) } else { None });
    // Graine du mode hasard : l'horloge murale suffit pour un exemple (la logique, elle, reçoit la graine injectée).
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let playlist = Playlist::new(tracks, start, seed);

    let output: Box<dyn OutputBackend> = match (options.null_speed, &options.device) {
        (Some(speed), _) => Box::new(NullOutput::new(48_000).with_speed(speed)),
        (None, Some(name)) => Box::new(CpalOutput::by_name(name)?),
        (None, None) => Box::new(CpalOutput::default_device()?),
    };
    let speed = options.null_speed.unwrap_or(1.0);
    let config = PlayerConfig {
        mode: options.mode,
        volume: options.volume,
        muted: options.muted,
    };
    let mut player = Player::new(playlist, config, output)?;
    let mut reader = FeedReader::new(player.take_analysis_feed().context("flux d'analyse")?)?;
    player.play();

    let started = Instant::now();
    let mut last_log = Instant::now();
    let mut last_track = None;
    let mut seen_last = false;
    let mut played_s = 0.0;
    let mut hop = [0.0_f32; HOP];
    let mut peak = 0.0_f32;
    loop {
        std::thread::sleep(Duration::from_millis(5));
        // Ce que ferait le thread d'analyse : lire le flux pas à pas (ici, on n'en garde que le niveau crête).
        while let Ok(HopStatus::Ready { .. }) = reader.next_hop(&mut hop, Instant::now()) {
            peak = hop.iter().fold(peak, |p, s| p.max(s.abs()));
        }
        let state = player.state();
        if state.track != last_track {
            if let Some(previous) = last_track {
                tracing::info!(from = previous, to = ?state.track, "changement de piste");
            }
            last_track = state.track;
            seen_last |= state.track == Some(count - 1);
            if options.once && seen_last && state.track == Some(0) {
                played_s = state.rendered_frames as f64 / f64::from(player.sample_rate());
                report(&player.state(), &reader, player.sample_rate());
                break;
            }
        }
        if last_log.elapsed() >= Duration::from_secs(1) {
            last_log = Instant::now();
            log_state(&state, peak);
            peak = 0.0;
        }
        if options
            .max_seconds
            .is_some_and(|max| started.elapsed().as_secs_f64() * speed >= max)
        {
            report(&state, &reader, player.sample_rate());
            break;
        }
    }
    if options.once {
        tracing::info!(
            played_s,
            wall_s = started.elapsed().as_secs_f64(),
            "tour complet de la playlist"
        );
    }
    Ok(())
}

fn log_state(state: &PlayerState, analysis_peak: f32) {
    tracing::info!(
        track = ?state.track,
        position = format!("{:.1}/{:.1} s", state.position_s, state.duration_s.unwrap_or(0.0)),
        underruns = state.underruns,
        device_errors = state.device_errors,
        analysis_peak = format!("{analysis_peak:.2}"),
        "lecture"
    );
}

fn report(state: &PlayerState, reader: &FeedReader, sample_rate: u32) {
    tracing::info!(
        rendered_s = state.rendered_frames as f64 / f64::from(sample_rate),
        underruns = state.underruns,
        device_errors = state.device_errors,
        analysis_written = reader.stats().written(),
        analysis_dropped = state.analysis_dropped,
        "bilan"
    );
}
