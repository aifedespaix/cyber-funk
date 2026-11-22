//! Outil `log` : délègue à `tools/debuglog/dbg.py` (aucune seconde lecture du journal ici).

use std::path::Path;
use std::process::{Command, Stdio};

use crate::process::workspace_root;

/// Sous-commandes de `dbg.py` autorisées.
const ALLOWED: [&str; 7] = ["crash", "summary", "timeline", "find", "spans", "check", "sessions"];
/// Nombre de lignes par défaut.
pub const DEFAULT_LIMIT: usize = 40;
/// Nombre de lignes maximal.
pub const MAX_LIMIT: usize = 400;

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
    command.current_dir(workspace_root()).stdin(Stdio::null());
    match command.output() {
        Err(error) => format!("exécution de dbg.py impossible ({python}) : {error}"),
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            if !output.status.success() {
                text.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            truncate_lines(&text, limit)
        }
    }
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
