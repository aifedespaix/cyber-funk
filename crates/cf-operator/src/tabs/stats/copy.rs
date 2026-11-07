//! « Copier pour un agent » : le texte collé dans une conversation avec un agent, mis en forme par
//! `cf_debuglog::format_for_agent` (en-tête de session, niveaux, filtre, format, puis les lignes de la plus ancienne à
//! la plus récente).
//!
//! Le contexte complet (événements actifs, quartier, plan, DJ…) est une autre variante : `SessionHeader::context`
//! reste vide ici.

use std::collections::BTreeSet;

use cf_debuglog::{Level, LogFilter, LogLine, SessionHeader, epoch_ms, format_for_agent, iso_utc};

use super::log_view::LogViewState;

/// Séparateur entre l'en-tête et les lignes, écrit par `format_for_agent`.
const BODY_SEPARATOR: &str = "---\n";

/// Les `copy_n` dernières lignes **visibles** : niveaux et filtre texte de l'affichage appliqués avant la coupe.
/// `lines` : les lignes affichées (la copie figée en pause).
pub(crate) fn copy_text(state: &LogViewState, lines: &[LogLine], header: &SessionHeader) -> String {
    format_for_agent(lines, &state.filter(), state.copy_n, header)
}

/// Les `copy_n` dernières lignes warn et error, quels que soient les niveaux et le filtre affichés : ce qu'un agent
/// regarde d'abord après un incident.
pub(crate) fn copy_warn_errors_text(state: &LogViewState, lines: &[LogLine], header: &SessionHeader) -> String {
    let filter = LogFilter {
        levels: BTreeSet::from([Level::Warn, Level::Error]),
        text: String::new(),
    };
    format_for_agent(lines, &filter, state.copy_n, header)
}

/// En-tête d'une copie faite maintenant (date UTC de l'horloge murale : c'est celle que l'agent comparera).
pub(crate) fn header(session: &str) -> SessionHeader {
    SessionHeader {
        session: session.to_owned(),
        copied_at: iso_utc(epoch_ms()),
        context: Vec::new(),
    }
}

/// Nombre de lignes de journal dans une copie (après l'en-tête), pour la confirmation « N lignes copiées ».
pub(crate) fn copied_lines(text: &str) -> usize {
    text.split_once(BODY_SEPARATOR)
        .map_or(0, |(_, body)| body.lines().count())
}
