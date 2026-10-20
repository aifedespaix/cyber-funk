//! Rejoue un set sans rendu et imprime la trace des changements (cues, appels, passations, drops, portes clés).
//!
//! ```text
//! cargo run -p cf-show --example simulate -- --show assets/shows/demo.ron --hours 3
//!   --assets <dossier>        dossier des assets (looks/, titles/) ; défaut : assets
//!   --bpm <n>                 tempo de la musique synthétique ; défaut : 125
//!   --seed <n>                graine du chef du spectacle ; défaut : 1
//!   --go-every <secondes>     envoie GO à intervalle régulier (pour les créneaux Semi et Manuel)
//!   --next-dj-every <s>       envoie NextDj à intervalle régulier
//! ```

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "outil en ligne de commande : la trace est sa sortie"
)]

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use cf_control::Action;
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_show::sim::{Command, Script, SyntheticMusic, simulate};
use rand_pcg::Pcg32;

struct Options {
    show: PathBuf,
    assets: PathBuf,
    hours: f64,
    bpm: f32,
    seed: u64,
    go_every: Option<f64>,
    next_dj_every: Option<f64>,
}

fn parse() -> Result<Options, String> {
    let mut options = Options {
        show: PathBuf::from("assets/shows/demo.ron"),
        assets: PathBuf::from("assets"),
        hours: 3.0,
        bpm: 125.0,
        seed: 1,
        go_every: None,
        next_dj_every: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} attend une valeur"));
        match flag.as_str() {
            "--show" => options.show = PathBuf::from(value("--show")?),
            "--assets" => options.assets = PathBuf::from(value("--assets")?),
            "--hours" => options.hours = number(&value("--hours")?, "--hours")?,
            "--bpm" => options.bpm = number(&value("--bpm")?, "--bpm")?,
            "--seed" => options.seed = number(&value("--seed")?, "--seed")?,
            "--go-every" => options.go_every = Some(number(&value("--go-every")?, "--go-every")?),
            "--next-dj-every" => options.next_dj_every = Some(number(&value("--next-dj-every")?, "--next-dj-every")?),
            other => return Err(format!("option inconnue : {other}")),
        }
    }
    Ok(options)
}

fn number<T: std::str::FromStr>(text: &str, name: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{name} : « {text} » n'est pas un nombre valide"))
}

/// Une action toutes les `every_s` secondes jusqu'à la fin du set.
fn repeat(mut script: Script, action: &Action, every_s: f64, duration_s: f64) -> Script {
    let mut t = every_s;
    while every_s > 0.0 && t < duration_s {
        script = script.act(t, action.clone());
        t += every_s;
    }
    script
}

fn run() -> Result<(), Box<dyn Error>> {
    let options = parse()?;
    let library = LookLibrary::load_dir(&options.assets.join("looks"))?;
    let titles = DropTitles::load(&options.assets.join("titles/drop-titles.ron"))?;
    let show = LoadedShow::load(&options.show, &library)?;
    let duration_s = options.hours * 3600.0;

    let mut script = Script::new().at(0.0, Command::Live(true));
    if let Some(every) = options.go_every {
        script = repeat(script, &Action::Go, every, duration_s);
    }
    if let Some(every) = options.next_dj_every {
        script = repeat(script, &Action::NextDj, every, duration_s);
    }
    let music = SyntheticMusic::club(options.bpm, true);

    let start = Instant::now();
    let report = simulate::<Pcg32>(&show, titles, options.seed, music, script, duration_s)?;
    let took = start.elapsed();
    println!("{}", report.trace.render());
    eprintln!(
        "{} h simulées ({} images) en {took:.2?} ; fin au créneau {}, cue {}",
        options.hours, report.trace.stats.frames, report.final_slot, report.final_cue
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("simulate : {error}");
            ExitCode::FAILURE
        }
    }
}
