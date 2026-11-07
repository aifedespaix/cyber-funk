//! L'enregistreur : file de messages, thread d'écriture, watchdog de gel, hook de panique.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use serde_json::{Map, Value, json};

use crate::time::{compact_utc, epoch_ms, iso_utc};

/// Nombre de sessions conservées sur le disque (les plus anciennes sont supprimées au démarrage).
const KEEP_SESSIONS: usize = 20;
/// Pas du watchdog (ms).
const WATCHDOG_TICK_MS: u64 = 250;
/// Longueur maximale d'une trace de panique écrite dans le journal (octets).
const MAX_BACKTRACE: usize = 8_000;
/// Variable d'environnement qui compte les relances automatiques (évite une boucle de relance infinie).
const RESTART_COUNT_VAR: &str = "CF_RESTART_COUNT";
/// Nombre maximal de relances automatiques d'affilée.
const MAX_RESTARTS: u32 = 3;

/// Gravité d'un fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Détail utile seulement en enquête.
    Debug,
    /// Fait normal du spectacle.
    Info,
    /// Anormal mais récupéré.
    Warn,
    /// Défaillance.
    Error,
}

impl Level {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// Phase d'un fait : un instant, ou le début / la fin d'un fait qui dure (`sid` les relie).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Fait ponctuel.
    Point,
    /// Début d'un fait qui dure.
    Start,
    /// Fin d'un fait qui dure.
    End,
}

impl Phase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

/// Un fait qui dure, rendu par [`Recorder::begin`] et à clore avec [`Recorder::end`].
#[derive(Debug, Clone)]
pub struct Span {
    sid: u64,
    ev: String,
    reference: Option<String>,
    started_up_ms: u64,
}

/// Réglages au démarrage de l'enregistreur.
#[derive(Debug, Clone)]
pub struct StartOptions {
    /// Une image qui n'arrive pas depuis ce délai (ms) est un gel, journalisé par le watchdog.
    pub stall_ms: u64,
    /// Relancer l'application si le gel dure ce délai (ms) ; `None` : seulement le journaliser.
    pub restart_after_ms: Option<u64>,
    /// Arguments à consigner dans `session.json`.
    pub args: Vec<String>,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self {
            stall_ms: 3_000,
            restart_after_ms: None,
            args: Vec::new(),
        }
    }
}

enum Msg {
    Line(String),
    Heartbeat(String),
    Flush(Sender<()>),
    Close,
}

struct Shared {
    tx: Sender<Msg>,
    seq: AtomicU64,
    span_seq: AtomicU64,
    t0: Instant,
    frame: AtomicU64,
    sim_ms: AtomicU64,
    /// Instant (ms depuis `t0`, plus 1) de la dernière image ; 0 tant qu'aucune image n'a eu lieu.
    last_frame_up: AtomicU64,
    records: AtomicU64,
    warns: AtomicU64,
    errors: AtomicU64,
    stalls: AtomicU64,
    stop: AtomicBool,
    dir: PathBuf,
}

/// Poignée de l'enregistreur : peu coûteuse à cloner, utilisable depuis n'importe quel thread **sauf** les callbacks
/// audio temps réel (règle 3 : zéro allocation, zéro verrou).
#[derive(Clone)]
pub struct Recorder {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("dir", &self.shared.dir)
            .finish_non_exhaustive()
    }
}

/// Garde de la session : à garder jusqu'à la fin de `main`. À sa destruction, la session est marquée `closed`.
#[must_use = "la garde doit vivre jusqu'à la fin du programme pour clore la session proprement"]
#[derive(Debug)]
pub struct Guard {
    recorder: Recorder,
    writer: Option<JoinHandle<()>>,
    watchdog: Option<JoinHandle<()>>,
    meta: Map<String, Value>,
}

impl Recorder {
    /// Ouvre une session dans `root` et démarre le thread d'écriture, le watchdog et le hook de panique.
    ///
    /// # Errors
    /// Si le dossier de session ne peut pas être créé ou si ses fichiers ne s'ouvrent pas.
    pub fn start(root: &Path, options: &StartOptions) -> std::io::Result<(Self, Guard)> {
        fs::create_dir_all(root)?;
        prune_old_sessions(root);
        let started = epoch_ms();
        let dir = root.join(format!("session-{}-{}", compact_utc(started), std::process::id()));
        fs::create_dir_all(&dir)?;
        let events = File::options()
            .create(true)
            .append(true)
            .open(dir.join("events.jsonl"))?;

        let (tx, rx) = crossbeam_channel::unbounded();
        let shared = Arc::new(Shared {
            tx,
            seq: AtomicU64::new(0),
            span_seq: AtomicU64::new(0),
            t0: Instant::now(),
            frame: AtomicU64::new(0),
            sim_ms: AtomicU64::new(0),
            last_frame_up: AtomicU64::new(0),
            records: AtomicU64::new(0),
            warns: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            stalls: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            dir: dir.clone(),
        });
        let recorder = Self { shared };

        let mut meta = Map::new();
        meta.insert("status".into(), json!("running"));
        meta.insert("started".into(), json!(iso_utc(started)));
        meta.insert("pid".into(), json!(std::process::id()));
        meta.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
        meta.insert("args".into(), json!(options.args));
        meta.insert("os".into(), json!(std::env::consts::OS));
        meta.insert("dir".into(), json!(dir.display().to_string()));
        write_json(&dir.join("session.json"), &Value::Object(meta.clone()));
        // Pointeur vers la session la plus récente : l'outil de lecture n'a ainsi rien à chercher.
        let latest = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = fs::write(root.join("LATEST.txt"), latest);

        let writer = thread::Builder::new().name("cf-debuglog-writer".into()).spawn({
            let dir = dir.clone();
            move || write_loop(&rx, events, &dir)
        })?;
        let watchdog = thread::Builder::new().name("cf-debuglog-watchdog".into()).spawn({
            let recorder = recorder.clone();
            let options = options.clone();
            move || recorder.watchdog_loop(&options)
        })?;
        recorder.install_panic_hook();
        recorder.point(
            Level::Info,
            "session.start",
            None,
            None,
            json!({ "args": options.args }),
        );
        Ok((
            recorder.clone(),
            Guard {
                recorder,
                writer: Some(writer),
                watchdog: Some(watchdog),
                meta,
            },
        ))
    }

    /// Dossier de la session.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.shared.dir
    }

    /// Millisecondes écoulées depuis le démarrage du processus.
    #[must_use]
    pub fn uptime_ms(&self) -> u64 {
        u64::try_from(self.shared.t0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Image courante et horloge de simulation (ms, pauses exclues) : copiées dans chaque fait. À appeler une fois par
    /// image ; sert aussi de signal de vie au watchdog.
    pub fn tick(&self, frame: u64, sim_ms: u64) {
        let s = &self.shared;
        s.frame.store(frame, Ordering::Relaxed);
        s.sim_ms.store(sim_ms, Ordering::Relaxed);
        s.last_frame_up.store(self.uptime_ms() + 1, Ordering::Relaxed);
    }

    /// Un fait ponctuel.
    pub fn point(&self, level: Level, ev: &str, reference: Option<&str>, label: Option<&str>, data: Value) {
        self.emit(level, Phase::Point, ev, None, reference, label, data);
    }

    /// Début d'un fait qui dure. Chaque fait doit être clos par [`end`](Self::end) ; l'outil de lecture signale les
    /// débuts sans fin.
    #[must_use]
    pub fn begin(&self, ev: &str, reference: Option<&str>, label: Option<&str>, data: Value) -> Span {
        let sid = self.shared.span_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.emit(Level::Info, Phase::Start, ev, Some(sid), reference, label, data);
        Span {
            sid,
            ev: ev.to_owned(),
            reference: reference.map(str::to_owned),
            started_up_ms: self.uptime_ms(),
        }
    }

    /// Fin d'un fait qui dure ; la durée réelle (ms) est ajoutée à `data`.
    pub fn end(&self, span: &Span, mut data: Value) {
        if let Value::Object(map) = &mut data {
            map.insert(
                "dur_ms".into(),
                json!(self.uptime_ms().saturating_sub(span.started_up_ms)),
            );
        }
        self.emit(
            Level::Info,
            Phase::End,
            &span.ev,
            Some(span.sid),
            span.reference.as_deref(),
            None,
            data,
        );
    }

    /// Réécrit `heartbeat.json` (dernier état connu). À appeler environ une fois par seconde.
    pub fn heartbeat(&self, mut state: Value) {
        if let Value::Object(map) = &mut state {
            let s = &self.shared;
            map.insert("wall".into(), json!(iso_utc(epoch_ms())));
            map.insert("up_ms".into(), json!(self.uptime_ms()));
            map.insert("frame".into(), json!(s.frame.load(Ordering::Relaxed)));
            map.insert("sim_ms".into(), json!(s.sim_ms.load(Ordering::Relaxed)));
            map.insert("records".into(), json!(s.records.load(Ordering::Relaxed)));
            map.insert("warns".into(), json!(s.warns.load(Ordering::Relaxed)));
            map.insert("errors".into(), json!(s.errors.load(Ordering::Relaxed)));
        }
        let _ = self.shared.tx.send(Msg::Heartbeat(state.to_string()));
    }

    /// Attend (au plus `max`) que tout ce qui a été envoyé soit écrit. Utile avant un arrêt brutal.
    pub fn flush(&self, max: Duration) {
        let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);
        if self.shared.tx.send(Msg::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(max);
        }
    }

    #[allow(clippy::too_many_arguments)] // Les champs d'une ligne du journal, rassemblés en un seul endroit.
    fn emit(
        &self,
        level: Level,
        phase: Phase,
        ev: &str,
        sid: Option<u64>,
        reference: Option<&str>,
        label: Option<&str>,
        data: Value,
    ) {
        let s = &self.shared;
        s.records.fetch_add(1, Ordering::Relaxed);
        match level {
            Level::Warn => {
                s.warns.fetch_add(1, Ordering::Relaxed);
            }
            Level::Error => {
                s.errors.fetch_add(1, Ordering::Relaxed);
            }
            Level::Debug | Level::Info => {}
        }
        let mut line = Map::new();
        line.insert("seq".into(), json!(s.seq.fetch_add(1, Ordering::Relaxed) + 1));
        line.insert("wall".into(), json!(iso_utc(epoch_ms())));
        line.insert("up".into(), json!(self.uptime_ms()));
        line.insert("sim".into(), json!(s.sim_ms.load(Ordering::Relaxed)));
        line.insert("frame".into(), json!(s.frame.load(Ordering::Relaxed)));
        line.insert("lvl".into(), json!(level.as_str()));
        line.insert("ph".into(), json!(phase.as_str()));
        line.insert("ev".into(), json!(ev));
        if let Some(sid) = sid {
            line.insert("sid".into(), json!(sid));
        }
        if let Some(reference) = reference {
            line.insert("ref".into(), json!(reference));
        }
        if let Some(label) = label {
            line.insert("label".into(), json!(label));
        }
        if !data.is_null() {
            line.insert("d".into(), data);
        }
        let _ = s.tx.send(Msg::Line(Value::Object(line).to_string()));
    }

    /// Surveille les images : un gel est journalisé (et, si demandé, l'application est relancée).
    fn watchdog_loop(&self, options: &StartOptions) {
        let s = &self.shared;
        let mut stalled_since: Option<u64> = None;
        while !s.stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(WATCHDOG_TICK_MS));
            let last = s.last_frame_up.load(Ordering::Relaxed);
            if last == 0 {
                continue; // Chargement : aucune image encore.
            }
            let gap = self.uptime_ms().saturating_sub(last - 1);
            match (gap >= options.stall_ms, stalled_since) {
                (true, None) => {
                    stalled_since = Some(last);
                    s.stalls.fetch_add(1, Ordering::Relaxed);
                    self.point(
                        Level::Error,
                        "watchdog.stall",
                        None,
                        Some("la boucle principale ne produit plus d'image"),
                        json!({ "gap_ms": gap, "last_frame": s.frame.load(Ordering::Relaxed) }),
                    );
                    self.flush(Duration::from_millis(200));
                }
                (true, Some(_)) => {
                    if options.restart_after_ms.is_some_and(|limit| gap >= limit) {
                        self.point(Level::Error, "watchdog.restart", None, None, json!({ "gap_ms": gap }));
                        self.flush(Duration::from_millis(500));
                        if relaunch_self().is_ok() {
                            std::process::exit(75);
                        }
                        stalled_since = Some(u64::MAX); // Relance impossible : on n'insiste pas.
                    }
                }
                (false, Some(since)) => {
                    stalled_since = None;
                    let ms = self.uptime_ms().saturating_sub(since.saturating_sub(1));
                    self.point(
                        Level::Warn,
                        "watchdog.recovered",
                        None,
                        None,
                        json!({ "stalled_ms": ms }),
                    );
                }
                (false, None) => {}
            }
        }
    }

    /// Consigne chaque panique (message, lieu, fil, trace) et vide la file avant que le processus ne tombe.
    fn install_panic_hook(&self) {
        let recorder = self.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info.location().map(|l| format!("{}:{}", l.file(), l.line()));
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let mut backtrace = std::backtrace::Backtrace::force_capture().to_string();
            backtrace.truncate(MAX_BACKTRACE);
            recorder.point(
                Level::Error,
                "panic",
                location.as_deref(),
                Some(&message),
                json!({ "thread": thread::current().name().unwrap_or("?"), "backtrace": backtrace }),
            );
            recorder.flush(Duration::from_millis(500));
            previous(info);
        }));
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let r = &self.recorder;
        r.point(Level::Info, "session.end", None, None, Value::Null);
        r.shared.stop.store(true, Ordering::Relaxed);
        let _ = r.shared.tx.send(Msg::Close);
        if let Some(handle) = self.writer.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.watchdog.take() {
            let _ = handle.join();
        }
        let s = &r.shared;
        self.meta.insert("status".into(), json!("closed"));
        self.meta.insert("ended".into(), json!(iso_utc(epoch_ms())));
        self.meta.insert("duration_ms".into(), json!(r.uptime_ms()));
        self.meta
            .insert("records".into(), json!(s.records.load(Ordering::Relaxed)));
        self.meta.insert("warns".into(), json!(s.warns.load(Ordering::Relaxed)));
        self.meta
            .insert("errors".into(), json!(s.errors.load(Ordering::Relaxed)));
        self.meta
            .insert("stalls".into(), json!(s.stalls.load(Ordering::Relaxed)));
        write_json(&s.dir.join("session.json"), &Value::Object(self.meta.clone()));
    }
}

/// Relance le même programme avec les mêmes arguments (le processus actuel doit ensuite se terminer).
///
/// # Errors
/// Si l'exécutable ne peut pas être localisé ou lancé, ou si trop de relances se sont déjà enchaînées.
pub fn relaunch_self() -> std::io::Result<()> {
    let count: u32 = std::env::var(RESTART_COUNT_VAR)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if count >= MAX_RESTARTS {
        return Err(std::io::Error::other("trop de relances automatiques d'affilée"));
    }
    std::process::Command::new(std::env::current_exe()?)
        .args(std::env::args().skip(1))
        .env(RESTART_COUNT_VAR, (count + 1).to_string())
        .spawn()
        .map(|_| ())
}

fn write_loop(rx: &Receiver<Msg>, mut events: File, dir: &Path) {
    loop {
        match rx.recv() {
            Ok(Msg::Line(mut line)) => {
                line.push('\n');
                // Pas de tampon : chaque ligne part au système tout de suite, elle survit donc à un plantage.
                let _ = events.write_all(line.as_bytes());
            }
            Ok(Msg::Heartbeat(text)) => {
                let tmp = dir.join("heartbeat.json.tmp");
                if fs::write(&tmp, text).is_ok() {
                    let _ = fs::rename(&tmp, dir.join("heartbeat.json"));
                }
            }
            Ok(Msg::Flush(ack)) => {
                let _ = ack.send(());
            }
            Ok(Msg::Close) | Err(_) => break,
        }
    }
}

fn write_json(path: &Path, value: &Value) {
    let _ = fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default());
}

fn prune_old_sessions(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    let mut sessions: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("session-"))
        })
        .collect();
    sessions.sort();
    let excess = sessions.len().saturating_sub(KEEP_SESSIONS - 1);
    for old in sessions.into_iter().take(excess) {
        let _ = fs::remove_dir_all(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_session_ecrit_ses_faits_et_se_ferme_proprement() {
        let root = tempfile::tempdir().unwrap();
        let (rec, guard) = Recorder::start(root.path(), &StartOptions::default()).unwrap();
        rec.tick(7, 1234);
        let span = rec.begin("cue.entered", Some("slot0/cue1"), Some("Intro"), json!({ "k": 1 }));
        rec.end(&span, json!({}));
        rec.heartbeat(json!({ "fps": 60 }));
        rec.flush(Duration::from_secs(1));
        let dir = rec.dir().to_path_buf();
        drop(guard);

        let lines: Vec<Value> = fs::read_to_string(dir.join("events.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let evs: Vec<_> = lines
            .iter()
            .map(|l| (l["ev"].as_str().unwrap(), l["ph"].as_str().unwrap()))
            .collect();
        assert_eq!(
            evs,
            [
                ("session.start", "point"),
                ("cue.entered", "start"),
                ("cue.entered", "end"),
                ("session.end", "point")
            ]
        );
        assert_eq!(lines[1]["sim"], 1234);
        assert_eq!(lines[1]["sid"], lines[2]["sid"]);
        assert!(lines[2]["d"]["dur_ms"].is_u64());
        let meta: Value = serde_json::from_str(&fs::read_to_string(dir.join("session.json")).unwrap()).unwrap();
        assert_eq!(meta["status"], "closed");
        assert!(dir.join("heartbeat.json").is_file());
        assert!(
            fs::read_to_string(root.path().join("LATEST.txt"))
                .unwrap()
                .starts_with("session-")
        );
    }

    #[test]
    fn le_watchdog_signale_un_gel_puis_la_reprise() {
        let root = tempfile::tempdir().unwrap();
        let options = StartOptions {
            stall_ms: 300,
            ..StartOptions::default()
        };
        let (rec, guard) = Recorder::start(root.path(), &options).unwrap();
        rec.tick(1, 0);
        thread::sleep(Duration::from_millis(900));
        rec.tick(2, 0);
        thread::sleep(Duration::from_millis(400));
        let dir = rec.dir().to_path_buf();
        drop(guard);
        let text = fs::read_to_string(dir.join("events.jsonl")).unwrap();
        assert!(text.contains("watchdog.stall"), "{text}");
        assert!(text.contains("watchdog.recovered"), "{text}");
    }

    #[test]
    fn seules_les_dernieres_sessions_sont_gardees() {
        let root = tempfile::tempdir().unwrap();
        for i in 0..(KEEP_SESSIONS + 5) {
            fs::create_dir_all(root.path().join(format!("session-{i:04}"))).unwrap();
        }
        prune_old_sessions(root.path());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), KEEP_SESSIONS - 1);
    }
}
