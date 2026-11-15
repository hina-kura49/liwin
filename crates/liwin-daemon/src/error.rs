// SPDX-License-Identifier: MIT OR Apache-2.0
//! D-Bus errors returned by the daemon (`io.github.liwin.Error.*`).

use zbus::DBusError;

#[derive(Debug, DBusError)]
#[zbus(prefix = "io.github.liwin.Error")]
pub enum LiwinError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// The command could not be parsed or is not allowed.
    InvalidCommand(String),
    /// No GNOME Shell extension is connected.
    ShellNotConnected(String),
    /// The extension did not acknowledge a request in time.
    Timeout(String),
    /// The caller may not use this interface.
    AccessDenied(String),
    /// The operation failed.
    Failed(String),
}

impl From<zbus::fdo::Error> for LiwinError {
    fn from(e: zbus::fdo::Error) -> Self {
        LiwinError::ZBus(e.into())
    }
}
