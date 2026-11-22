//! Lancement de l'application : on démarre un binaire DÉJÀ compilé (jamais `cargo run` : tuer cargo ne tuerait pas
//! l'application, sous Windows). Attente de `probe.json`, fin de stderr conservée pour expliquer un échec de
//! démarrage, mort du processus surveillée, et enfant tué à la destruction.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, bail};

/// Nom du fichier d'état écrit par l'application.
pub const STATE_FILE: &str = "probe.json";
/// Délai de démarrage par défaut.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
/// Plafond du délai de démarrage (évite tout débordement d'`Instant`).
pub const MAX_STARTUP_TIMEOUT: Duration = Duration::from_secs(600);
/// Nombre de lignes de stderr conservées pour les messages d'erreur.
const STDERR_TAIL_LINES: usize = 40;
/// Pas d'interrogation pendant l'attente du démarrage.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Commande de compilation à indiquer quand aucun binaire n'existe ou qu'il est sans la sonde.
const BUILD_HINT: &str = "cargo build -p cyber-funk --features probe";

/// Options de `launch`.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Show à jouer (`--show`).
    pub show: Option<PathBuf>,
    /// Épingle le pas de temps des `Step` (`CF_PROBE_STEP_S=0.05`). Non vérifié sur l'application réelle.
    pub fixed_step: bool,
    /// Rendu logiciel : pas de drapeau dans l'application, on pose `WGPU_FORCE_FALLBACK_ADAPTER=1`. Non vérifié sur
    /// l'application réelle.
    pub software_render: bool,
    /// Arguments supplémentaires passés tels quels à l'application.
    pub extra_args: Vec<String>,
}

/// Commande prête à lancer, avec le binaire retenu (pour le montrer à l'appelant).
#[derive(Debug)]
pub struct Resolved {
    /// Commande à lancer.
    pub command: Command,
    /// Binaire choisi.
    pub binary: PathBuf,
    /// Date de dernière modification du binaire.
    pub modified: SystemTime,
}

impl Resolved {
    /// Phrase qui dit quel binaire est lancé et de quand il date (il peut être périmé).
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "binaire {} (modifié le {} UTC)",
            self.binary.display(),
            format_utc(self.modified)
        )
    }
}

/// Application lancée, avec le port de sa sonde (0 tant qu'elle n'a pas démarré). Tuée à la destruction.
#[derive(Debug)]
pub struct Launched {
    /// Processus enfant.
    pub child: Child,
    /// Port d'écoute de la sonde, lu dans `probe.json`.
    pub port: u16,
    tail: Arc<Mutex<VecDeque<String>>>,
    reader: Option<JoinHandle<()>>,
}

/// Où en est le démarrage.
#[derive(Debug)]
pub enum StartState {
    /// `probe.json` pas encore là.
    Pending,
    /// La sonde écoute sur ce port.
    Ready(u16),
    /// Le processus est mort ; message prêt à afficher.
    Failed(String),
}

impl Launched {
    /// Lance `command` : supprime d'abord un `probe.json` périmé (on ne s'attache jamais à un ancien run), jette le
    /// stdout de l'enfant (celui du serveur est réservé à MCP) et garde la fin de son stderr.
    ///
    /// # Errors
    /// Dossier d'état inutilisable ou lancement impossible.
    pub fn spawn(mut command: Command, state_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(state_dir).context("création du dossier d'état")?;
        match std::fs::remove_file(state_dir.join(STATE_FILE)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("suppression d'un probe.json périmé"),
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("lancement de l'application")?;
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let reader = child.stderr.take().map(|stderr| {
            let tail = Arc::clone(&tail);
            std::thread::spawn(move || drain_stderr(stderr, &tail))
        });
        Ok(Self {
            child,
            port: 0,
            tail,
            reader,
        })
    }

    /// Fin de la sortie d'erreur de l'application (40 lignes au plus).
    #[must_use]
    pub fn stderr_tail(&self) -> String {
        let tail = self.tail.lock().unwrap_or_else(PoisonError::into_inner);
        tail.iter().map(String::as_str).collect::<Vec<_>>().join("\n")
    }

    /// `Some(statut)` si le processus est terminé (surveillance par `try_wait`, à chaque appel d'outil).
    pub fn exited(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Tue le processus et attend sa fin.
    pub fn kill(&mut self) {
        // Erreurs ignorées : le processus peut déjà être mort, ce qui est le but.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Un pas de l'attente du démarrage.
    pub fn poll_start(&mut self, state_dir: &Path) -> StartState {
        if let Some(port) = read_port(state_dir) {
            self.port = port;
            return StartState::Ready(port);
        }
        match self.exited() {
            Some(status) => {
                self.settle_stderr();
                StartState::Failed(exit_message(status, &self.stderr_tail()))
            }
            None => StartState::Pending,
        }
    }

    /// Tue l'application qui n'a pas annoncé sa sonde à temps et rend le message d'erreur.
    pub fn timeout_failure(&mut self, timeout: Duration) -> String {
        self.kill();
        self.settle_stderr();
        format!(
            "probe.json absent après {} s : application tuée. Fin de stderr :\n{}",
            timeout.as_secs(),
            self.stderr_tail()
        )
    }

    /// Laisse au lecteur de stderr le temps de vider le tube d'un processus mort.
    fn settle_stderr(&mut self) {
        if let Some(reader) = self.reader.take() {
            let deadline = Instant::now() + Duration::from_millis(500);
            while !reader.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if reader.is_finished() {
                // Un lecteur qui a paniqué n'enlève rien d'utile au message d'erreur.
                let _ = reader.join();
            }
        }
    }
}

impl Drop for Launched {
    /// L'application ne survit pas au serveur MCP (EOF sur stdin, plantage du client...).
    fn drop(&mut self) {
        self.kill();
    }
}

/// Message d'un démarrage raté ; explique le cas du binaire compilé sans la feature `probe`.
fn exit_message(status: ExitStatus, stderr_tail: &str) -> String {
    let hint = if stderr_tail.contains("unexpected argument '--probe'") {
        format!(
            "\nCe binaire a été compilé sans la feature `probe` (ou est périmé) : le recompiler avec `{BUILD_HINT}`."
        )
    } else {
        String::new()
    };
    format!("l'application s'est arrêtée au démarrage ({status}). Fin de stderr :\n{stderr_tail}{hint}")
}

/// Attend que l'application rangée dans `slot` annonce sa sonde. Le verrou n'est tenu que le temps d'un pas : `stop`
/// (ou un nouveau `launch`) peut retirer l'application pendant l'attente, ce qui la fait échouer proprement.
///
/// # Errors
/// Application retirée, morte au démarrage ou délai dépassé (alors tuée).
pub fn wait_for_probe(
    slot: &Mutex<Option<Launched>>,
    pid: u32,
    state_dir: &Path,
    timeout: Duration,
) -> anyhow::Result<u16> {
    let timeout = timeout.min(MAX_STARTUP_TIMEOUT);
    let deadline = Instant::now() + timeout;
    loop {
        {
            let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(launched) = guard.as_mut().filter(|launched| launched.child.id() == pid) else {
                bail!("lancement interrompu : `stop` ou un nouveau `launch` a retiré l'application");
            };
            match launched.poll_start(state_dir) {
                StartState::Ready(port) => return Ok(port),
                StartState::Failed(message) => {
                    *guard = None;
                    bail!(message);
                }
                StartState::Pending if Instant::now() >= deadline => {
                    let message = launched.timeout_failure(timeout);
                    *guard = None;
                    bail!(message);
                }
                StartState::Pending => {}
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Lit le port dans `probe.json` ; `None` tant que le fichier manque ou n'est pas complet.
#[must_use]
pub fn read_port(dir: &Path) -> Option<u16> {
    let content = std::fs::read_to_string(dir.join(STATE_FILE)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    u16::try_from(value.get("port")?.as_u64()?).ok()
}

/// Racine du dépôt (là où vivent `Cargo.toml` et `tools/`).
#[must_use]
pub fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(2).unwrap_or(manifest).to_path_buf()
}

/// Binaire `cyber-funk` le plus récent de `<target>/{debug,release}`, avec sa date.
fn built_binary(target: &Path) -> Option<(PathBuf, SystemTime)> {
    let name = format!("cyber-funk{}", std::env::consts::EXE_SUFFIX);
    ["debug", "release"]
        .iter()
        .map(|profile| target.join(profile).join(&name))
        .filter_map(|path| Some((path.clone(), std::fs::metadata(&path).ok()?.modified().ok()?)))
        .max_by_key(|(_, modified)| *modified)
}

/// Commande de lancement d'après `opts`, avec le binaire du dossier cible du dépôt (`CARGO_TARGET_DIR` sinon `target`).
///
/// # Errors
/// Aucun binaire compilé : le message donne la commande de compilation.
pub fn build_command(opts: &LaunchOptions) -> anyhow::Result<Resolved> {
    let root = workspace_root();
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    build_command_in(&root, &target, opts)
}

/// Comme [`build_command`], dossiers choisis (les tests y mettent un dossier temporaire).
///
/// # Errors
/// Aucun binaire compilé dans `target`.
pub fn build_command_in(root: &Path, target: &Path, opts: &LaunchOptions) -> anyhow::Result<Resolved> {
    let Some((binary, modified)) = built_binary(target) else {
        bail!(
            "aucun binaire cyber-funk compilé dans {} : lancer d'abord `{BUILD_HINT}`",
            target.display()
        );
    };
    let mut command = Command::new(&binary);
    command.arg("--probe").current_dir(root);
    if let Some(show) = &opts.show {
        command.arg("--show").arg(show);
    }
    command.args(&opts.extra_args);
    if opts.fixed_step {
        command.env("CF_PROBE_STEP_S", "0.05");
    }
    if opts.software_render {
        command.env("WGPU_FORCE_FALLBACK_ADAPTER", "1");
    }
    Ok(Resolved {
        command,
        binary,
        modified,
    })
}

/// Lance l'application d'après `opts` et attend sa sonde (délai de démarrage par défaut).
///
/// # Errors
/// Pas de binaire compilé, démarrage raté ou délai dépassé.
// Interface du contrat de la tâche ; le serveur range l'enfant lui-même pour que `stop` puisse l'interrompre.
#[allow(dead_code)]
pub fn launch(opts: &LaunchOptions, state_dir: &Path) -> anyhow::Result<Launched> {
    let launched = Launched::spawn(build_command(opts)?.command, state_dir)?;
    let pid = launched.child.id();
    let slot = Mutex::new(Some(launched));
    wait_for_probe(&slot, pid, state_dir, DEFAULT_STARTUP_TIMEOUT)?;
    slot.into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .context("application disparue après son démarrage")
}

/// Lit stderr jusqu'à la fermeture du tube en gardant les dernières lignes (et en le vidant : l'enfant ne bloque pas).
fn drain_stderr(stderr: std::process::ChildStderr, tail: &Mutex<VecDeque<String>>) {
    use std::io::{BufRead as _, BufReader};
    for line in BufReader::new(stderr).lines() {
        let Ok(line) = line else { break };
        let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
        if tail.len() == STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

/// `AAAA-MM-JJ HH:MM:SS` en UTC (algorithme de Hinnant, sans dépendance de calendrier).
fn format_utc(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0));
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Le processus `pid` existe-t-il encore ? (tests seulement)
#[cfg(test)]
pub fn is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&format!(" {pid} ")))
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success())
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
