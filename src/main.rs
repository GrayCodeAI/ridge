#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::uninlined_format_args)]
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod agents;
mod app;
mod cli;
mod config;
mod core;
mod event;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Logging to file (never stdout — TUI owns stdout).
    let log_dir = config::config_dir();
    std::fs::create_dir_all(&log_dir)?;
    let file_appender = tracing_appender::rolling::never(&log_dir, "term.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("term=info".parse()?))
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    tracing::info!("term starting (version {})", env!("CARGO_PKG_VERSION"));

    let config = config::Config::load()?;

    if cli.api {
        let app = std::sync::Arc::new(std::sync::Mutex::new(
            crate::app::App::new(config),
        ));
        crate::core::api_server::start_server(app)?;
        return Ok(());
    }

    if cli.update {
        return crate::core::update::run_update(env!("CARGO_PKG_VERSION"));
    }

    app::run(config, cli).await
}
