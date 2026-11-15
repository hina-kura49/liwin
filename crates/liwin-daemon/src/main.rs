use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use liwin_daemon::{Options, service};
use tokio::signal::unix::{SignalKind, signal};

/// liwin daemon: owns io.github.liwin.Daemon on the session bus.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Accept `debug ...` commands such as `debug move-window`.
    #[arg(long)]
    enable_debug_commands: bool,
    /// Milliseconds to wait for the extension to acknowledge a command.
    #[arg(long, default_value_t = 2000)]
    request_timeout_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LIWIN_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    let options = Options {
        enable_debug_commands: args.enable_debug_commands,
        request_timeout: Duration::from_millis(args.request_timeout_ms),
    };

    let _conn = service::serve(zbus::connection::Builder::session(), options)
        .await
        .context("failed to start the D-Bus service (is another liwin-daemon running?)")?;
    tracing::info!("serving {} on the session bus", liwin_daemon::BUS_NAME);

    let mut term = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutting down");
    Ok(())
}
