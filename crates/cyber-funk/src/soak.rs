//! Mode `--soak <heures>` (J9-01) : la démo tourne en boucle avec des actions d'opérateur aléatoires, et une ligne
//! de mesures est écrite dans un CSV toutes les `SAMPLE_EVERY_S` secondes. Le programme se ferme à l'heure dite.
//!
//! Colonnes : temps, mémoire du processus (Mo), entités, images, images > 50 ms, pire image de la fenêtre (ms),
//! créneau, cue, numéro de passation. Les images lentes pendant une passation se repèrent avec `slot_seq`, qui change.
//! La mémoire GPU n'est pas lisible de façon portable : elle se relève à part (outil du constructeur) pendant le test.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use bevy::prelude::*;
use cf_control::{Action, Switch, TimedAction};
use cf_render::{CfSet, ControlAction, CurrentShow};
use sysinfo::{Pid, ProcessesToUpdate, System};

/// Seuil d'une image jugée trop longue (critère de la fiche J9-01).
const SLOW_FRAME_S: f32 = 0.050;
/// Période des lignes du CSV.
const SAMPLE_EVERY_S: f64 = 10.0;
/// Période moyenne entre deux actions d'opérateur.
const ACTION_EVERY_S: f64 = 2.0;

/// Branche le mode d'endurance.
#[derive(Debug, Clone)]
pub struct SoakPlugin {
    /// Durée du test, en heures.
    pub hours: f64,
    /// Fichier CSV écrit.
    pub csv: PathBuf,
}

impl Plugin for SoakPlugin {
    fn build(&self, app: &mut App) {
        match Recorder::open(self.hours, &self.csv) {
            Ok(recorder) => {
                tracing::info!(hours = self.hours, csv = %self.csv.display(), "mode endurance actif");
                app.insert_resource(recorder)
                    .add_systems(Update, (inject_actions.in_set(CfSet::Input), record));
            }
            Err(error) => {
                // Un test d'endurance qui n'écrit rien ne sert à rien : on le dit fort, sans faire planter l'app.
                tracing::error!(%error, "mode endurance désactivé");
            }
        }
    }
}

#[derive(Resource)]
struct Recorder {
    out: BufWriter<File>,
    system: System,
    pid: Pid,
    end_s: f64,
    next_sample_s: f64,
    next_action_s: f64,
    rng: u64,
    frames: u64,
    slow: u64,
    worst_s: f32,
}

impl Recorder {
    fn open(hours: f64, path: &PathBuf) -> anyhow::Result<Self> {
        let file = File::create(path).map_err(|e| anyhow::anyhow!("CSV {} : {e}", path.display()))?;
        let mut out = BufWriter::new(file);
        writeln!(out, "t_s,mem_mb,entities,frames,slow_frames,worst_ms,slot,cue,slot_seq")?;
        Ok(Self {
            out,
            system: System::new()?,
            pid: sysinfo::get_current_pid().map_err(|e| anyhow::anyhow!("pid du processus : {e}"))?,
            end_s: hours * 3600.0,
            next_sample_s: SAMPLE_EVERY_S,
            next_action_s: ACTION_EVERY_S,
            rng: 0x9E37_79B9_7F4A_7C15,
            frames: 0,
            slow: 0,
            worst_s: 0.0,
        })
    }

    /// xorshift64 : suffisant pour varier les actions, sans dépendance de plus.
    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn memory_mb(&mut self) -> f64 {
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
        self.system
            .process(self.pid)
            .map_or(0.0, |p| p.memory() as f64 / (1024.0 * 1024.0))
    }
}

/// Une action d'opérateur tirée au sort : surtout des GO, rarement une passation.
fn pick(roll: u64) -> Option<Action> {
    match roll % 100 {
        0..=9 => Some(Action::Go),
        10..=12 => Some(Action::Back),
        13..=15 => Some(Action::ForceDrop),
        16 => Some(Action::NextDj),
        17 => Some(Action::PrevDj),
        18 => Some(Action::SkipShot),
        19 => Some(Action::Hold(Switch::Toggle)),
        _ => None,
    }
}

fn inject_actions(mut recorder: ResMut<Recorder>, time: Res<Time<Real>>, mut out: MessageWriter<ControlAction>) {
    let now = time.elapsed_secs_f64();
    if now < recorder.next_action_s {
        return;
    }
    recorder.next_action_s = now + ACTION_EVERY_S;
    let roll = recorder.next();
    if let Some(action) = pick(roll) {
        out.write(ControlAction(TimedAction { time_s: now, action }));
    }
}

fn record(
    mut recorder: ResMut<Recorder>,
    time: Res<Time<Real>>,
    show: Option<Res<CurrentShow>>,
    entities: Query<Entity>,
    mut exit: MessageWriter<AppExit>,
) {
    let dt = time.delta_secs();
    recorder.frames += 1;
    recorder.worst_s = recorder.worst_s.max(dt);
    if dt > SLOW_FRAME_S {
        recorder.slow += 1;
    }
    let now = time.elapsed_secs_f64();
    if now < recorder.next_sample_s {
        return;
    }
    recorder.next_sample_s = now + SAMPLE_EVERY_S;
    let mem = recorder.memory_mb();
    let (slot, cue, seq) = show.map_or((0, 0, 0), |s| (s.slot, s.cue, s.slot_seq));
    let (frames, slow, worst_ms) = (recorder.frames, recorder.slow, recorder.worst_s * 1000.0);
    let count = entities.iter().count();
    let line = writeln!(
        recorder.out,
        "{now:.1},{mem:.1},{count},{frames},{slow},{worst_ms:.1},{slot},{cue},{seq}"
    );
    // Vidé à chaque ligne : une coupure en pleine nuit ne doit pas coûter les heures déjà mesurées.
    if let Err(error) = line.and_then(|()| recorder.out.flush()) {
        tracing::error!(%error, "écriture du CSV d'endurance impossible");
    }
    recorder.frames = 0;
    recorder.slow = 0;
    recorder.worst_s = 0.0;
    if now >= recorder.end_s {
        tracing::info!("endurance terminée");
        exit.write(AppExit::Success);
    }
}
