//! Serveur MCP de la sonde : parle MCP sur stdin/stdout, et JSON-par-ligne en TCP local avec l'application.
//!
//! stdout est le transport MCP : rien d'autre n'y est jamais écrit, les journaux vont sur stderr.

mod client;
mod logs;
mod mcp;
mod process;

use anyhow::Context as _;
use rmcp::ServiceExt as _;
use rmcp::transport::stdio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let server = mcp::ProbeServer::new(mcp::default_state_dir());
    let service = server.serve(stdio()).await.context("démarrage du serveur MCP")?;
    service.waiting().await.context("serveur MCP interrompu")?;
    Ok(())
}
