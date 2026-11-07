//! Le journal de session vu par l'opérateur : la poignée du tampon circulaire de `cf-debuglog`, et le repère
//! « Marquer ce moment ».
//!
//! Le tampon est rempli par l'enregistreur (`Recorder::emit`, donc exactement ce qu'écrit `events.jsonl`) ; l'interface
//! ne relit jamais le fichier. Sans enregistreur (`--soak` sans disque, tests, journal impossible à ouvrir) la
//! ressource est absente et l'onglet dit « Journal indisponible ».

use std::borrow::Cow;

use bevy::prelude::*;
use cf_debuglog::{Level, LogBuffer, LogLine, Recorder};

/// Nom de l'événement d'un repère posé par l'opérateur (`ev` dans le journal).
pub(crate) const MARK_EV: &str = "mark";

/// Session affichée et copiée quand l'enregistreur n'est pas joignable.
pub(crate) const UNKNOWN_SESSION: &str = "session inconnue";

/// Le tampon des dernières lignes du journal de session, posé par le binaire (`Recorder::log_buffer()`).
#[derive(Resource, Debug, Clone)]
pub struct OperatorLog(pub LogBuffer);

/// Pousse un repère (`ev = "mark"`, niveau info) directement dans le tampon, sans passer par l'enregistreur.
///
/// Le tampon ne connaît ni l'horloge ni le compteur de l'enregistreur : le repère reprend le numéro, l'horodatage et
/// l'image de la dernière ligne connue (numéro + 1), de quoi le situer entre ses voisines. C'est le repli quand aucun
/// enregistreur n'est branché ; sinon [`mark`] passe par lui, et le repère rejoint aussi `events.jsonl`.
pub fn mark_moment(buffer: &LogBuffer, label: &str) {
    let last = buffer.snapshot().pop();
    let (seq, uptime_ms, frame) = last.map_or((1, 0, 0), |l| (l.seq + 1, l.uptime_ms, l.frame));
    buffer.push(LogLine {
        seq,
        uptime_ms,
        frame,
        level: Level::Info,
        ev: MARK_EV.to_owned(),
        label: label.to_owned(),
    });
}

/// Pose un repère : par l'enregistreur s'il est là (fichier + tampon, numéro et horloge exacts), sinon dans le seul
/// tampon. Sans l'un ni l'autre, rien (il n'y a pas de journal à marquer).
pub(crate) fn mark(recorder: Option<&Recorder>, buffer: Option<&LogBuffer>, label: &str) {
    match (recorder, buffer) {
        // `Default::default()` est le `Value::Null` de `serde_json` (« sans détail »), sans ajouter la dépendance ici.
        #[allow(clippy::default_trait_access)]
        (Some(recorder), _) => recorder.point(Level::Info, MARK_EV, None, Some(label), Default::default()),
        (None, Some(buffer)) => mark_moment(buffer, label),
        (None, None) => {}
    }
}

/// Nom de la session (dossier `session-<date>-<pid>` de l'enregistreur), ou [`UNKNOWN_SESSION`].
pub(crate) fn session_name(recorder: Option<&Recorder>) -> Cow<'_, str> {
    recorder
        .and_then(|r| r.dir().file_name())
        .map_or(Cow::Borrowed(UNKNOWN_SESSION), |n| n.to_string_lossy())
}
