// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use liwin_daemon::{Options, config, service};
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
    /// Config file [default: $XDG_CONFIG_HOME/liwin/liwin.toml].
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LIWIN_LOG")
                // zbus spans would otherwise prefix every line with the full
                // D-Bus message being dispatched.
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,zbus=warn")),
        )
        .init();
    let args = Args::parse();

    let config_path = args
        .config
        .or_else(config::default_path)
        .context("cannot determine the config path (set $HOME or pass --config)")?;
    let loaded = config::load(&config_path).map_err(anyhow::Error::msg)?;
    let (
        config::Settings {
            keybindings,
            layout,
        },
        warnings,
    ) = loaded.settings();
    for w in &warnings {
        tracing::warn!("{}: {w}", config_path.display());
    }
    if loaded.builtin {
        tracing::info!(
            "{} not found; using the built-in config ({} keybindings)",
            config_path.display(),
            keybindings.len()
        );
    } else {
        tracing::info!(
            "config {} loaded ({} keybindings)",
            config_path.display(),
            keybindings.len()
        );
    }

    let options = Options {
        enable_debug_commands: args.enable_debug_commands,
        request_timeout: Duration::from_millis(args.request_timeout_ms),
        keybindings,
        layout,
        config_path: Some(config_path),
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
