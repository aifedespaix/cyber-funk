//! Outil `log` : délègue à `tools/debuglog/dbg.py` (aucune seconde lecture du journal ici).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::process::workspace_root;

/// Sous-commandes de `dbg.py` autorisées.
const ALLOWED: [&str; 7] = ["crash", "summary", "timeline", "find", "spans", "check", "sessions"];
/// Nombre de lignes par défaut.
pub const DEFAULT_LIMIT: usize = 40;
/// Nombre de lignes maximal.
pub const MAX_LIMIT: usize = 400;
/// Délai de garde de dbg.py.
const RUN_TIMEOUT: Duration = Duration::from_secs(20);
/// Octets de stdout lus au plus (~1 Mio) avant de tronquer.
const MAX_OUTPUT_BYTES: usize = 1 << 20;
/// Octets de stderr lus au plus.
const STDERR_BYTES: usize = 16 << 10;

/// Arguments de `log`.
#[derive(Debug, Clone)]
pub struct LogArgs {
    /// Sous-commande de `dbg.py` (liste blanche).
    pub subcommand: String,
    /// Arguments propres à la sous-commande (`--around 83500`, une référence...), passés sans shell.
    pub args: Vec<String>,
    /// Nombre de lignes de sortie au plus.
    pub limit: usize,
    /// Dossier des sessions (`--root`).
    pub root: Option<String>,
    /// Session voulue (`--session`).
    pub session: Option<String>,
}

/// Interpréteur Python de la plate-forme.
#[must_use]
pub fn python_program() -> String {
    if cfg!(windows) { "python" } else { "python3" }.to_owned()
}

/// Exécute le `dbg.py` du dépôt. Rend toujours un texte, erreur comprise : l'outil ne plante jamais.
#[must_use]
pub fn run(args: &LogArgs) -> String {
    let script = workspace_root().join("tools").join("debuglog").join("dbg.py");
    run_with(&python_program(), &script, args)
}

/// Comme [`run`], avec l'interpréteur et le script choisis (les tests y mettent un faux script).
#[must_use]
pub fn run_with(python: &str, script: &Path, args: &LogArgs) -> String {
    run_with_limits(python, script, args, RUN_TIMEOUT, MAX_OUTPUT_BYTES)
}

/// Comme [`run_with`], avec le délai de garde et la taille maximale lue (les tests les réduisent).
#[must_use]
pub fn run_with_limits(python: &str, script: &Path, args: &LogArgs, timeout: Duration, max_bytes: usize) -> String {
    if !ALLOWED.contains(&args.subcommand.as_str()) {
        return format!(
            "sous-commande `{}` refusée ; autorisées : {}",
            args.subcommand,
            ALLOWED.join(", ")
        );
    }
    let limit = args.limit.clamp(1, MAX_LIMIT);
    let mut command = Command::new(python);
    command.arg(script).arg(&args.subcommand).args(&args.args);
    command.arg("--limit").arg(limit.to_string());
    if let Some(root) = &args.root {
        command.arg("--root").arg(root);
    }
    if let Some(session) = &args.session {
        command.arg("--session").arg(session);
    }
    // Sous Windows Python écrit sinon en cp1252 quand sa sortie est un tube.
    command.env("PYTHONIOENCODING", "utf-8");
    command
        .current_dir(workspace_root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return format!("exécution de dbg.py impossible ({python}) : {error}"),
    };
    let stdout = child.stdout.take().map(|pipe| read_bounded(pipe, max_bytes));
    let stderr = child.stderr.take().map(|pipe| read_bounded(pipe, STDERR_BYTES));
    let deadline = Instant::now() + timeout;
    let mut problem = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => {
                problem = Some(format!("attente de dbg.py impossible : {error}"));
                break None;
            }
        }
        if stdout.as_ref().is_some_and(BoundedRead::is_capped) {
            problem = Some(format!(
                "[sortie de dbg.py bornée à {max_bytes} octets : processus arrêté]"
            ));
            break None;
        }
        if Instant::now() >= deadline {
            problem = Some(format!("dbg.py a dépassé {} s : processus tué", timeout.as_secs()));
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if status.is_none() {
        // Erreurs ignorées : le processus a pu finir entre-temps.
        let _ = child.kill();
        let _ = child.wait();
    }
    let mut text = stdout.map(BoundedRead::join).unwrap_or_default();
    let failed = status.is_some_and(|status| !status.success());
    if failed
        || problem
            .as_deref()
            .is_some_and(|text| text.starts_with("dbg.py a dépassé"))
    {
        text.push('\n');
        text.push_str(&stderr.map(BoundedRead::join).unwrap_or_default());
    }
    let mut out = truncate_lines(&text, limit);
    if let Some(problem) = problem {
        out.push('\n');
        out.push_str(&problem);
    }
    out
}

/// Lecture d'un tube dans un fil, bornée à `max` octets (le reste n'est jamais chargé en mémoire).
struct BoundedRead {
    handle: std::thread::JoinHandle<Vec<u8>>,
    capped: Arc<AtomicBool>,
}

impl BoundedRead {
    /// La borne est atteinte : le processus a probablement encore à écrire, il faut l'arrêter.
    fn is_capped(&self) -> bool {
        self.capped.load(Ordering::Relaxed)
    }

    fn join(self) -> String {
        let bytes = self.handle.join().unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn read_bounded(pipe: impl Read + Send + 'static, max: usize) -> BoundedRead {
    let capped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&capped);
    let handle = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let limit = u64::try_from(max).unwrap_or(u64::MAX);
        // Erreur de lecture : on garde ce qui est déjà lu.
        let _ = pipe.take(limit).read_to_end(&mut bytes);
        flag.store(bytes.len() >= max, Ordering::Relaxed);
        bytes
    });
    BoundedRead { handle, capped }
}

/// Garde `limit` lignes au plus ; si on tronque, la dernière dit combien manquent.
fn truncate_lines(text: &str, limit: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= limit {
        return lines.join("\n");
    }
    let mut kept: Vec<String> = lines[..limit - 1].iter().map(|line| (*line).to_owned()).collect();
    kept.push(format!("[... {} lignes tronquées]", lines.len() - (limit - 1)));
    kept.join("\n")
}
