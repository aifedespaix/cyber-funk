//! Sonde de test : un canal de contrôle local pour le serveur MCP (voir `docs/superpowers/specs/`).
//!
//! ```text
//! client TCP ─▶ thread du serveur (server.rs) ─ Pending{requête, réponse} ─▶ canal mpsc
//!                                                                              │ try_recv
//!                                       système Bevy `CfSet::Input` (bridge.rs) ◀┘ ─▶ message ControlAction
//! ```
//!
//! Le thread réseau ne touche jamais au monde Bevy. La sonde n'écoute que si l'application est lancée avec
//! `--probe`, et ses actions passent par le même chemin que le clavier : le limiteur photosensible les voit.

mod bridge;
mod server;
#[cfg(test)]
mod tests;

use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;

use bevy::prelude::*;
use cf_render::CfSet;

use crate::logging;

/// Nom du fichier d'état, dans le dossier de session : `{ "port": u16, "pid": u32 }`.
pub const STATE_FILE: &str = "probe.json";

/// Active la sonde : ouvre le port local et branche le pont vers les messages `ControlAction`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProbePlugin;

impl Plugin for ProbePlugin {
    fn build(&self, app: &mut App) {
        // Une sonde qui ne démarre pas ne doit pas empêcher de jouer : on journalise et on continue sans elle.
        let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!(%error, "sonde : port local indisponible, la sonde reste éteinte");
                return;
            }
        };
        let port = match listener.local_addr() {
            Ok(address) => address.port(),
            Err(error) => {
                tracing::error!(%error, "sonde : adresse locale illisible, la sonde reste éteinte");
                return;
            }
        };
        if let Some(dir) = logging::default_log_dir() {
            if let Err(error) = write_state_file(&dir, port) {
                tracing::warn!(%error, "sonde : {STATE_FILE} non écrit, le serveur MCP ne trouvera pas le port");
            }
        } else {
            tracing::warn!("sonde : pas de dossier de session, {STATE_FILE} non écrit");
        }
        let (inbox, _handle) = server::spawn(listener);
        tracing::info!(port, "sonde de test à l'écoute sur 127.0.0.1");
        app.insert_resource(bridge::ProbeInbox::new(inbox))
            .add_systems(Update, bridge::drain_probe.in_set(CfSet::Input));
    }
}

/// Écrit `probe.json` dans `dir` (créé au besoin) : le port d'écoute et le pid, pour que le serveur MCP trouve
/// l'application et sache si le fichier est resté d'un lancement précédent.
///
/// # Errors
/// Si le dossier ou le fichier ne peut pas être écrit.
pub fn write_state_file(dir: &Path, port: u16) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let content = serde_json::json!({ "port": port, "pid": std::process::id() });
    std::fs::write(dir.join(STATE_FILE), content.to_string())
}
