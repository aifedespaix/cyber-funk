//! Tampon circulaire des dernières lignes du journal, et leur mise en forme « pour un agent » (copie dans le
//! presse-papiers de l'opérateur).

use std::collections::{BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::recorder::Level;

/// Capacité par défaut du tampon (lignes).
pub const DEFAULT_CAPACITY: usize = 500;

/// Une ligne du journal, telle que gardée en mémoire.
#[derive(Debug, Clone)]
pub struct LogLine {
    /// Numéro de la ligne dans la session (identique à `seq` dans `events.jsonl`).
    pub seq: u64,
    /// Millisecondes depuis le démarrage du processus.
    pub uptime_ms: u64,
    /// Image courante au moment du fait.
    pub frame: u64,
    /// Gravité.
    pub level: Level,
    /// Nom de l'événement.
    pub ev: String,
    /// Détail lisible (vide s'il n'y en a pas).
    pub label: String,
}

struct Inner {
    capacity: usize,
    lines: VecDeque<LogLine>,
}

impl Inner {
    fn trim(&mut self) {
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
        }
    }
}

/// Poignée partagée sur le tampon : se clone, s'utilise depuis n'importe quel thread **sauf** les callbacks audio.
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<Inner>>,
}

impl std::fmt::Debug for LogBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogBuffer").finish_non_exhaustive()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl LogBuffer {
    /// Tampon qui garde au plus `capacity` lignes.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                capacity,
                lines: VecDeque::with_capacity(capacity),
            })),
        }
    }

    // Un verrou empoisonné ne rend pas les données invalides ici : on les reprend plutôt que de paniquer.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Ajoute une ligne ; la plus ancienne est oubliée si le tampon est plein.
    pub fn push(&self, line: LogLine) {
        let mut inner = self.lock();
        inner.lines.push_back(line);
        inner.trim();
    }

    /// Copie des lignes, de la plus ancienne à la plus récente.
    #[must_use]
    pub fn snapshot(&self) -> Vec<LogLine> {
        self.lock().lines.iter().cloned().collect()
    }

    /// Vide le tampon.
    pub fn clear(&self) {
        self.lock().lines.clear();
    }

    /// Change la capacité ; si elle diminue, les plus anciennes lignes sont oubliées.
    pub fn set_capacity(&self, capacity: usize) {
        let mut inner = self.lock();
        inner.capacity = capacity;
        inner.trim();
    }
}

/// Filtre d'affichage : niveaux admis (vide = tous) et texte cherché dans `ev` et `label` (vide = tout).
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Niveaux admis ; un ensemble vide admet tous les niveaux.
    pub levels: BTreeSet<Level>,
    /// Texte cherché sans tenir compte de la casse.
    pub text: String,
}

impl LogFilter {
    /// La ligne passe-t-elle le filtre ?
    #[must_use]
    pub fn matches(&self, line: &LogLine) -> bool {
        if !self.levels.is_empty() && !self.levels.contains(&line.level) {
            return false;
        }
        if self.text.is_empty() {
            return true;
        }
        let needle = self.text.to_lowercase();
        line.ev.to_lowercase().contains(&needle) || line.label.to_lowercase().contains(&needle)
    }
}

/// En-tête de la copie : identité de la session et contexte (paires clé / valeur) fourni par l'appelant.
#[derive(Debug, Clone, Default)]
pub struct SessionHeader {
    /// Identifiant de la session.
    pub session: String,
    /// Date de la copie (texte déjà formaté).
    pub copied_at: String,
    /// Contexte libre, repris tel quel dans l'ordre.
    pub context: Vec<(String, String)>,
}

fn format_line(line: &LogLine) -> String {
    #[allow(clippy::cast_precision_loss)] // Affichage en secondes : la perte de précision est sans importance.
    let up_s = line.uptime_ms as f64 / 1000.0;
    format!(
        "#{} +{up_s:.3}s f{} {:<5} {} {}",
        line.seq,
        line.frame,
        line.level.label(),
        line.ev,
        line.label
    )
    .trim_end()
    .to_owned()
}

/// Texte à coller à un agent : en-tête, puis les `last` dernières lignes qui passent `filter` (le filtre est appliqué
/// **avant** de couper aux `last` dernières).
#[must_use]
pub fn format_for_agent(lines: &[LogLine], filter: &LogFilter, last: usize, header: &SessionHeader) -> String {
    let kept: Vec<&LogLine> = lines.iter().filter(|l| filter.matches(l)).collect();
    let shown = &kept[kept.len().saturating_sub(last)..];

    let levels = if filter.levels.is_empty() {
        "tous".to_owned()
    } else {
        filter.levels.iter().map(|l| l.label()).collect::<Vec<_>>().join(", ")
    };
    let text = if filter.text.is_empty() {
        "aucun".to_owned()
    } else {
        format!("\"{}\"", filter.text)
    };

    let mut out = String::new();
    let _ = writeln!(out, "session: {}", header.session);
    let _ = writeln!(out, "copié le: {}", header.copied_at);
    for (key, value) in &header.context {
        let _ = writeln!(out, "{key}: {value}");
    }
    let _ = writeln!(out, "niveaux: {levels}");
    let _ = writeln!(out, "filtre: {text}");
    let _ = writeln!(out, "lignes: {} sur {}", shown.len(), lines.len());
    out.push_str("format: #seq +uptime frame NIVEAU événement détail\n---\n");
    for line in shown {
        out.push_str(&format_line(line));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(seq: u64, level: Level, ev: &str, label: &str) -> LogLine {
        LogLine {
            seq,
            uptime_ms: seq * 1000,
            frame: seq * 10,
            level,
            ev: ev.into(),
            label: label.into(),
        }
    }

    fn header() -> SessionHeader {
        SessionHeader {
            session: "session-X".into(),
            copied_at: "2026-11-07T10:00:00Z".into(),
            context: vec![("cue".into(), "slot0/cue1".into())],
        }
    }

    fn seqs(lines: &[LogLine]) -> Vec<u64> {
        lines.iter().map(|l| l.seq).collect()
    }

    #[test]
    fn ring_keeps_last_capacity_in_order() {
        let buf = LogBuffer::new(3);
        for i in 1..=5 {
            buf.push(line(i, Level::Info, "e", ""));
        }
        assert_eq!(seqs(&buf.snapshot()), [3, 4, 5]);
    }

    #[test]
    fn set_capacity_shrinks_from_the_oldest() {
        let buf = LogBuffer::new(5);
        for i in 1..=5 {
            buf.push(line(i, Level::Info, "e", ""));
        }
        buf.set_capacity(2);
        assert_eq!(seqs(&buf.snapshot()), [4, 5]);
        buf.clear();
        assert!(buf.snapshot().is_empty());
    }

    #[test]
    fn filter_matches_level_and_text_case_insensitive() {
        let l = line(1, Level::Warn, "Watchdog.Stall", "Boucle BLOQUÉE");
        let mut f = LogFilter::default();
        assert!(f.matches(&l));
        f.levels.insert(Level::Error);
        assert!(!f.matches(&l));
        f.levels.insert(Level::Warn);
        assert!(f.matches(&l));
        f.text = "watchdog".into();
        assert!(f.matches(&l));
        f.text = "bloquée".into();
        assert!(f.matches(&l));
        f.text = "absent".into();
        assert!(!f.matches(&l));
    }

    #[test]
    fn format_takes_last_n_after_filter() {
        let lines = [
            line(1, Level::Error, "a", ""),
            line(2, Level::Info, "b", ""),
            line(3, Level::Error, "c", ""),
            line(4, Level::Info, "d", ""),
            line(5, Level::Error, "e", ""),
        ];
        let mut f = LogFilter::default();
        f.levels.insert(Level::Error);
        let out = format_for_agent(&lines, &f, 2, &header());
        assert!(!out.contains("#1 "), "{out}");
        assert!(out.contains("#3 "), "{out}");
        assert!(out.contains("#5 "), "{out}");
        assert!(!out.contains("#4 "), "{out}");
    }

    #[test]
    fn format_with_empty_buffer_has_header_and_no_lines() {
        let out = format_for_agent(&[], &LogFilter::default(), 50, &header());
        assert!(out.contains("session: session-X"), "{out}");
        assert!(out.contains("cue: slot0/cue1"), "{out}");
        assert!(out.ends_with("---\n"), "{out}");
    }

    #[test]
    fn format_with_n_larger_than_len_returns_all() {
        let lines = [line(1, Level::Info, "a", "x"), line(2, Level::Info, "b", "y")];
        let out = format_for_agent(&lines, &LogFilter::default(), 999, &header());
        assert!(out.contains("#1 ") && out.contains("#2 "), "{out}");
        assert!(out.contains("lignes: 2 sur 2"), "{out}");
    }

    #[test]
    fn format_header_lists_levels_and_filter() {
        let lines = [line(2, Level::Warn, "watchdog.stall", "gel")];
        let mut f = LogFilter::default();
        f.levels.insert(Level::Warn);
        f.levels.insert(Level::Error);
        f.text = "watch".into();
        let out = format_for_agent(&lines, &f, 10, &header());
        assert!(out.contains("session: session-X"), "{out}");
        assert!(out.contains("copié le: 2026-11-07T10:00:00Z"), "{out}");
        assert!(out.contains("niveaux: WARN, ERROR"), "{out}");
        assert!(out.contains("filtre: \"watch\""), "{out}");
        assert!(out.contains("#2 +2.000s f20 WARN  watchdog.stall gel\n"), "{out}");
    }
}
