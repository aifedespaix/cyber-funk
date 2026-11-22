//! Lancement de l'application : binaire déjà compilé s'il existe, sinon `cargo run`. Attend `probe.json`, garde la
//! fin de la sortie d'erreur (pour expliquer un échec de démarrage) et surveille la mort du processus.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};

/// Nom du fichier d'état écrit par l'application.
pub const STATE_FILE: &str = "probe.json";
/// Délai de démarrage par défaut : large, la première compilation de Bevy est longue.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
/// Nombre de lignes de stderr conservées pour les messages d'erreur.
const STDERR_TAIL_LINES: usize = 40;
/// Pas d'interrogation pendant l'attente du démarrage.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Options de `launch`.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Show à jouer (`--show`).
    pub show: Option<PathBuf>,
    /// Épingle le pas de temps des `Step` à celui de la répétition (`CF_PROBE_STEP_S=0.05`) ; l'application n'a pas
    /// d'autre réglage, et son pas par défaut est déjà fixe.
    pub fixed_step: bool,
    /// Rendu logiciel : l'application n'a pas de drapeau pour cela, on positionne la variable de wgpu
    /// `WGPU_FORCE_FALLBACK_ADAPTER=1` (adaptateur de repli, ex. WARP ou llvmpipe).
    pub software_render: bool,
    /// Arguments supplémentaires passés tels quels à l'application.
    pub extra_args: Vec<String>,
}

/// Application lancée, avec le port de sa sonde.
#[derive(Debug)]
pub struct Launched {
    /// Processus enfant.
    pub child: Child,
    /// Port d'écoute de la sonde, lu dans `probe.json`.
    pub port: u16,
    tail: Arc<Mutex<VecDeque<String>>>,
    reader: Option<JoinHandle<()>>,
}

impl Launched {
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

/// Binaire `cyber-funk` déjà compilé (le plus récent de `debug`/`release`), s'il existe.
fn built_binary(root: &Path) -> Option<PathBuf> {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let name = format!("cyber-funk{}", std::env::consts::EXE_SUFFIX);
    ["debug", "release"]
        .iter()
        .map(|profile| target.join(profile).join(&name))
        .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// Commande de lancement de l'application d'après `opts`.
#[must_use]
pub fn build_command(opts: &LaunchOptions) -> Command {
    let root = workspace_root();
    let mut command = if let Some(binary) = built_binary(&root) {
        let mut command = Command::new(binary);
        command.arg("--probe");
        command
    } else {
        let mut command = Command::new("cargo");
        command.args(["run", "-p", "cyber-funk", "--features", "probe", "--", "--probe"]);
        command
    };
    command.current_dir(&root);
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
    command
}

/// Lance l'application d'après `opts` et attend sa sonde (délai de démarrage par défaut).
///
/// # Errors
/// Voir [`launch_command`].
// Interface du contrat de la tâche ; le serveur passe par `build_command` + `launch_command` (commande injectable).
#[allow(dead_code)]
pub fn launch(opts: &LaunchOptions, state_dir: &Path) -> anyhow::Result<Launched> {
    launch_command(build_command(opts), state_dir, DEFAULT_STARTUP_TIMEOUT)
}

/// Lance `command` et attend que `probe.json` apparaisse dans `state_dir`.
///
/// Un `probe.json` laissé par un lancement précédent est supprimé AVANT le démarrage : on ne s'attache jamais à
/// une ancienne exécution. Le stdout de l'enfant est jeté (celui du serveur est réservé à MCP).
///
/// # Errors
/// Impossible de lancer, processus mort au démarrage ou délai dépassé : le message contient la fin de stderr.
pub fn launch_command(mut command: Command, state_dir: &Path, startup_timeout: Duration) -> anyhow::Result<Launched> {
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
    let mut launched = Launched {
        child,
        port: 0,
        tail,
        reader,
    };
    let deadline = Instant::now() + startup_timeout;
    loop {
        if let Some(port) = read_port(state_dir) {
            launched.port = port;
            return Ok(launched);
        }
        if let Some(status) = launched.exited() {
            launched.settle_stderr();
            bail!(
                "l'application s'est arrêtée au démarrage ({status}). Fin de stderr :\n{}",
                launched.stderr_tail()
            );
        }
        if Instant::now() >= deadline {
            launched.kill();
            launched.settle_stderr();
            bail!(
                "probe.json absent après {} s : application tuée. Fin de stderr :\n{}",
                startup_timeout.as_secs(),
                launched.stderr_tail()
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }
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
