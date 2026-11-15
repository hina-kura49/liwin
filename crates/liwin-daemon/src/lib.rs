//! liwin daemon library: the D-Bus service and the window model behind it.
//!
//! The binary (`main.rs`) only parses arguments and connects to the session
//! bus; everything else lives here so it can be exercised by integration tests
//! against a private `dbus-daemon`.

pub mod bridge;
pub mod commands;
pub mod config;
pub mod error;
pub mod model;
pub mod service;

use std::time::Duration;

pub use error::LiwinError;

/// Well-known bus name owned by the daemon.
pub const BUS_NAME: &str = "io.github.liwin.Daemon";
/// Object path of the daemon's single object.
pub const OBJECT_PATH: &str = "/io/github/liwin/Daemon";
/// Public interface used by the CLI.
pub const DAEMON_INTERFACE: &str = "io.github.liwin.Daemon1";
/// Interface reserved for the GNOME Shell extension.
pub const BRIDGE_INTERFACE: &str = "io.github.liwin.ShellBridge1";
/// A name owned by GNOME Shell itself on the connection its extensions use.
/// Its owner is the only peer allowed to use [`BRIDGE_INTERFACE`].
pub const SHELL_IDENTITY_NAME: &str = "org.gnome.Shell.Introspect";
/// Version of the extension ⇄ daemon protocol (see `docs/dbus.md`).
pub const PROTOCOL_VERSION: u32 = 1;

/// Runtime options of the daemon.
#[derive(Debug, Clone)]
pub struct Options {
    /// Accept `debug ...` commands (e.g. `debug move-window`).
    pub enable_debug_commands: bool,
    /// How long to wait for the extension to acknowledge a command.
    pub request_timeout: Duration,
    /// Keybindings of the `main` mode, sent to the extension.
    pub keybindings: Vec<config::KeyBinding>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enable_debug_commands: false,
            request_timeout: Duration::from_secs(2),
            keybindings: Vec::new(),
        }
    }
}
