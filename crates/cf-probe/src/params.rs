//! Paramètres des outils MCP (leur schéma JSON est dérivé de ces types).

use schemars::JsonSchema;
use serde::Deserialize;

/// Paramètres de `send_action`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendActionParams {
    /// Action au format JSON de `cf_control::Action` (serde par défaut : `"Go"`, `{"Cue": 3}`...).
    pub action: serde_json::Value,
}

/// Paramètres de `capture`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureParams {
    /// Nom du fichier image, sans dossier.
    pub name: String,
}

/// Paramètres de `step`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StepParams {
    /// Nombre d'images à avancer : de 1 à 1200 (0 est refusé, au-delà l'application plafonne).
    pub frames: u32,
}

/// Paramètres de `launch`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LaunchParams {
    /// Show à jouer (`--show`), chemin d'un fichier RON.
    pub show: Option<String>,
    /// Épingle le pas de temps des `step` (`CF_PROBE_STEP_S`) ; non vérifié sur l'application réelle.
    pub fixed_step: Option<bool>,
    /// Rendu logiciel (variable wgpu `WGPU_FORCE_FALLBACK_ADAPTER=1`) ; non vérifié sur l'application réelle.
    pub software_render: Option<bool>,
    /// Arguments supplémentaires de l'application.
    pub extra_args: Option<Vec<String>>,
    /// Délai de démarrage en secondes (120 par défaut, 600 au plus).
    pub startup_timeout_s: Option<u64>,
}

/// Paramètres de `log`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogParams {
    /// Sous-commande de dbg.py : crash, summary, timeline, find, spans, check ou sessions.
    pub subcommand: String,
    /// Arguments de la sous-commande (ex. `--around 83500 --window 3000`, ou une référence `slot1/cue2`).
    pub args: Option<Vec<String>>,
    /// Nombre de lignes au plus (40 par défaut, 400 au maximum).
    pub limit: Option<usize>,
    /// Session voulue (la dernière par défaut).
    pub session: Option<String>,
}
