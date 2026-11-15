// SPDX-License-Identifier: MIT OR Apache-2.0
//! Config reload: re-read the config file and apply the new keybindings and
//! layout settings to a running daemon.
//!
//! Not reachable from `RunCommand` yet; exposing it (e.g. as
//! `reload-config`) is a command-language change.

use std::path::Path;

use zbus::Connection;

use crate::LiwinError;
use crate::bridge::{ShellBridgeSignals, keybinding_table};
use crate::config::{self, Settings};
use crate::service::{Shared, unicast};

/// What a reload changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReloadOutcome {
    /// The keybinding table differs from the previous one.
    pub keybindings_changed: bool,
    /// Gaps or the default root orientation differ.
    pub layout_changed: bool,
    /// The keybinding table must be re-sent to the connected Shell (it has
    /// one already and the table changed).
    pub resend_keybindings: bool,
    /// The config file did not exist and the built-in config was used.
    pub builtin: bool,
    /// Warnings from loading and resolving the config.
    pub warnings: Vec<String>,
}

impl Shared {
    /// Replaces the live settings. Trees are marked for re-layout when the
    /// layout settings changed; nothing is sent to the Shell.
    pub fn apply_settings(&self, settings: Settings) -> ReloadOutcome {
        let mut st = self.state();
        let layout_changed = st.model.set_settings(settings.layout);
        let keybindings_changed = st.keybindings != settings.keybindings;
        st.keybindings = settings.keybindings;
        ReloadOutcome {
            keybindings_changed,
            layout_changed,
            // Before the first snapshot, SyncWindows sends the new table.
            resend_keybindings: keybindings_changed
                && st.shell_peer.is_some()
                && st.keybindings_sent,
            ..ReloadOutcome::default()
        }
    }

    /// Re-reads `path` (a missing file means the built-in config) and
    /// applies it: the new keybinding table is sent to the Shell if needed
    /// (the extension replaces its grabs and answers `KeybindingsApplied`),
    /// and changed gaps re-lay out every tree. On a load error the running
    /// settings are kept.
    pub async fn reload_config_from(
        &self,
        conn: &Connection,
        path: &Path,
    ) -> Result<ReloadOutcome, LiwinError> {
        let loaded = config::load(path).map_err(LiwinError::Failed)?;
        let (settings, warnings) = loaded.settings();
        let mut outcome = self.apply_settings(settings);
        outcome.builtin = loaded.builtin;
        outcome.warnings = warnings;
        if outcome.resend_keybindings {
            let (peer, table) = {
                let st = self.state();
                (st.shell_peer.clone(), keybinding_table(&st.keybindings))
            };
            if let Some(peer) = peer {
                unicast(conn, peer)?.keybindings_changed(table).await?;
            }
        }
        self.flush_layout(conn).await?;
        Ok(outcome)
    }

    /// [`Self::reload_config_from`] the config file the daemon started with.
    pub async fn reload_config(&self, conn: &Connection) -> Result<ReloadOutcome, LiwinError> {
        let path = self
            .options
            .config_path
            .clone()
            .ok_or_else(|| LiwinError::Failed("the daemon has no config file path".into()))?;
        self.reload_config_from(conn, &path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;
    use crate::config::KeyBinding;
    use crate::model::{LayoutSettings, RootOrientation};

    fn binding(key: &str) -> KeyBinding {
        KeyBinding {
            key: key.into(),
            accelerator: format!("<Alt>{key}"),
            commands: vec![vec!["focus".into(), "left".into()]],
        }
    }

    #[test]
    fn apply_settings_reports_changes() {
        let shared = Shared::new(Options {
            keybindings: vec![binding("h")],
            ..Options::default()
        });
        assert!(shared.options.keybindings.is_empty(), "moved into State");
        assert_eq!(shared.state().keybindings, vec![binding("h")]);

        let same = Settings {
            keybindings: vec![binding("h")],
            layout: LayoutSettings::default(),
        };
        assert_eq!(shared.apply_settings(same), ReloadOutcome::default());

        let other = Settings {
            keybindings: vec![binding("j")],
            layout: LayoutSettings {
                root_orientation: RootOrientation::Vertical,
                ..LayoutSettings::default()
            },
        };
        let outcome = shared.apply_settings(other);
        assert!(outcome.keybindings_changed && outcome.layout_changed);
        assert!(!outcome.resend_keybindings, "no Shell connected");
        let st = shared.state();
        assert_eq!(st.keybindings, vec![binding("j")]);
        assert_eq!(
            st.model.settings().root_orientation,
            RootOrientation::Vertical
        );
    }
}
