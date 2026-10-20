//! La boucle de simulation : musique et scénario en entrée, chef du spectacle au milieu, trace en sortie.

use cf_control::Action;
use rand_core::{Rng, SeedableRng};

use super::{Recorder, Script, SyntheticMusic, Trace};
use crate::director::{DirectorError, ShowDirector, ShowInput};
use crate::drop::DropTitles;
use crate::load::LoadedShow;

/// Pas de temps de la simulation : une image à 60 Hz.
const DT: f32 = 1.0 / 60.0;
/// Heure locale au début du set (s depuis minuit) : 22 h, pour les avancées `At`.
const START_LOCAL_S: f64 = 22.0 * 3600.0;

/// Pourquoi une simulation n'a pas pu démarrer.
#[derive(Debug, thiserror::Error)]
pub enum SimError {
    /// Le chef du spectacle n'a pas pu ouvrir l'événement.
    #[error(transparent)]
    Director(#[from] DirectorError),
}

/// Résultat d'une simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct SimReport {
    /// Ce qui s'est passé.
    pub trace: Trace,
    /// Créneau en cours à la fin du set.
    pub final_slot: usize,
    /// Cue en cours à la fin du set.
    pub final_cue: usize,
}

/// Rejoue `duration_s` secondes de set au pas de 1/60 s, avec le générateur `R` (`rand_pcg::Pcg32` par exemple).
///
/// # Errors
///
/// [`SimError::Director`] si le chef du spectacle ne peut pas ouvrir l'événement.
pub fn simulate<R: Rng + SeedableRng + Send + Sync + 'static>(
    show: &LoadedShow,
    titles: DropTitles,
    seed: u64,
    mut music: SyntheticMusic,
    mut script: Script,
    duration_s: f64,
) -> Result<SimReport, SimError> {
    let mut director = ShowDirector::<R>::new(show, titles, seed)?;
    let mut recorder = Recorder::new(show.event());
    let mut events = Vec::with_capacity(64);
    events.extend(director.drain_events());
    recorder.observe(0.0, &director, &events);

    let mut actions: Vec<Action> = Vec::with_capacity(16);
    let frames = (duration_s / f64::from(DT)).round() as u64;
    for frame in 1..=frames {
        let t = frame as f64 * f64::from(DT);
        script.due(t, &mut music, &mut actions);
        let heard = music.frame(DT);
        let input = ShowInput {
            dt: DT,
            now: t,
            local_time_s: (START_LOCAL_S + t).rem_euclid(86_400.0),
            audio: &heard.audio,
            real_drop: heard.real_drop,
            actions: &actions,
        };
        director.update(&input);
        actions.clear();
        events.clear();
        events.extend(director.drain_events());
        recorder.observe(t, &director, &events);
    }
    let state = director.state();
    Ok(SimReport {
        final_slot: state.slot,
        final_cue: state.cue,
        trace: recorder.finish(),
    })
}
