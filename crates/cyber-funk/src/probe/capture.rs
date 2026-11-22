//! Capture d'écran et pas de temps fixe demandés par la sonde.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use cf_probe_proto::ProbeReply;

/// Dossier des captures de la sonde, relatif au dossier de travail.
pub const CAPTURE_DIR: &str = "captures/probe";
/// Pas de temps d'une image pendant un `Step` (s) : le même que `--rehearse`.
const DEFAULT_STEP_S: f32 = 0.05;
/// Longueur maximale d'un nom de capture.
const MAX_NAME_LEN: usize = 64;
/// Plafond d'images d'un `Step` (1 min à 20 images/s de pas fixe) : un client ne peut pas figer le temps réel pour
/// toujours, ce qui rend la restauration de la stratégie de temps réelle.
pub const MAX_STEP_FRAMES: u32 = 1200;
/// Noms réservés par Windows : ils désignent un périphérique, quel que soit le suffixe.
const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Dossier d'écriture des captures (ressource optionnelle : [`CAPTURE_DIR`] sans elle).
#[derive(Resource, Debug, Clone)]
pub struct CaptureDir(pub PathBuf);

/// Valide le nom d'une capture et rend le chemin du fichier dans `dir`.
///
/// Un chemin (`/`, `\`, `..`) est refusé plutôt que nettoyé : écrire ailleurs que dans le dossier de la sonde serait
/// une faille, et un nom silencieusement changé tromperait le client. Les autres caractères hors `[A-Za-z0-9_-]` sont
/// remplacés par `_`.
///
/// # Errors
/// Nom vide, trop long, ou contenant un séparateur de chemin ou `..`.
pub fn capture_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err(format!(
            "nom de capture vide ou plus long que {MAX_NAME_LEN} caractères"
        ));
    }
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err(format!("nom de capture avec chemin refusé : {name}"));
    }
    if WINDOWS_RESERVED
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
    {
        return Err(format!("nom réservé par Windows refusé : {name}"));
    }
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(dir.join(format!("{clean}.png")))
}

/// Demande la capture de la fenêtre principale. La capture est asynchrone (écrite quelques images plus tard par le
/// moteur de rendu) : on répond tout de suite avec le chemin cible.
pub fn request_capture(commands: &mut Commands, dir: Option<&CaptureDir>, name: &str) -> ProbeReply {
    let default_dir = PathBuf::from(CAPTURE_DIR);
    let dir = dir.map_or(default_dir.as_path(), |d| d.0.as_path());
    match capture_path(dir, name) {
        Ok(path) => {
            let shown = path.to_string_lossy().replace('\\', "/");
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            ProbeReply::Captured { path: shown }
        }
        Err(why) => ProbeReply::Error(why),
    }
}

/// Un `Step` en cours : la réponse part quand `remaining` images se sont écoulées.
#[derive(Resource, Default)]
pub struct StepState {
    remaining: u32,
    reply: Option<Sender<ProbeReply>>,
    previous: Option<TimeUpdateStrategy>,
}

impl std::fmt::Debug for StepState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `TimeUpdateStrategy` n'implémente pas `Debug` : on n'en montre que la présence.
        f.debug_struct("StepState")
            .field("remaining", &self.remaining)
            .field("waiting", &self.reply.is_some())
            .finish_non_exhaustive()
    }
}

/// Pas de temps d'une image : `CF_PROBE_STEP_S` s'il est valide, sinon 0,05 s.
fn step_duration() -> Duration {
    let step_s = std::env::var("CF_PROBE_STEP_S")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(DEFAULT_STEP_S);
    Duration::from_secs_f32(step_s)
}

/// Démarre un `Step` de `frames` images à pas fixe ; `Some(réponse)` si la réponse est immédiate (refus ou rien à faire).
pub fn begin_step(
    state: &mut StepState,
    strategy: &mut TimeUpdateStrategy,
    frames: u32,
    reply: Sender<ProbeReply>,
) -> Option<(Sender<ProbeReply>, ProbeReply)> {
    if state.reply.is_some() {
        return Some((reply, ProbeReply::Error("un pas est déjà en cours".into())));
    }
    // Refus avant toute modification de la stratégie de temps.
    if frames == 0 || frames > MAX_STEP_FRAMES {
        let why = format!("nombre d'images hors de 1..={MAX_STEP_FRAMES} : {frames}");
        return Some((reply, ProbeReply::Error(why)));
    }
    // Le pas fixe vaut à partir de l'image suivante : la requête arrive en cours d'image, son temps est déjà écoulé.
    state.previous = Some(std::mem::replace(
        strategy,
        TimeUpdateStrategy::ManualDuration(step_duration()),
    ));
    state.remaining = frames;
    state.reply = Some(reply);
    None
}

/// Compte les images du `Step` en cours, répond et rend le temps au rythme réel à la dernière. À placer avant le vidage
/// de la sonde : une image passée depuis l'ordre est comptée avant d'en lire un nouveau.
pub fn tick_step(mut state: ResMut<StepState>, mut strategy: Option<ResMut<TimeUpdateStrategy>>) {
    if state.reply.is_none() {
        return;
    }
    state.remaining = state.remaining.saturating_sub(1);
    if state.remaining > 0 {
        return;
    }
    if let (Some(previous), Some(strategy)) = (state.previous.take(), strategy.as_deref_mut()) {
        *strategy = previous;
    }
    if let Some(reply) = state.reply.take() {
        // Le client a pu expirer : sa réponse n'a plus de destinataire.
        let _ = reply.send(ProbeReply::Ack);
    }
}
