// SPDX-License-Identifier: MIT OR Apache-2.0
//! Config reload (internal API; not exposed as a command yet).

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::*;
use liwin_daemon::Options;
use liwin_daemon::service::{self, Shared};

/// A config file in a private temporary directory, removed on drop.
struct TempConfig {
    dir: PathBuf,
}

impl TempConfig {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("liwin-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempConfig { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("liwin.toml")
    }

    fn write(&self, text: &str) {
        std::fs::write(self.path(), text).unwrap();
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn daemon_with(bus: &Bus, config: &TempConfig) -> (zbus::Connection, Arc<Shared>) {
    let loaded = liwin_daemon::config::load(&config.path()).unwrap();
    let (settings, _) = loaded.settings();
    let shared = Arc::new(Shared::new(Options {
        keybindings: settings.keybindings,
        layout: settings.layout,
        config_path: Some(config.path()),
        ..Options::default()
    }));
    let conn = service::serve_shared(
        zbus::connection::Builder::address(bus.address.as_str()),
        shared.clone(),
    )
    .await
    .unwrap();
    (conn, shared)
}

#[tokio::test]
async fn reload_resends_bindings_and_relayouts() {
    let bus = Bus::start().await;
    let config = TempConfig::new("reload");
    config.write("[mode.main.binding]\nalt-h = 'workspace 2'\n");
    let (daemon, shared) = daemon_with(&bus, &config).await;
    let shell = bus.shell().await;
    let mut tables = bridge_signals(&shell, "KeybindingsChanged").await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    let mut focus = bridge_signals(&shell, "FocusWindow").await;
    hello_and_sync(&shell).await;
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert_eq!(table, vec![(0, "<Alt>h".to_owned())]);
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    // Unchanged file: nothing to do.
    let outcome = shared.reload_config(&daemon).await.unwrap();
    assert!(!outcome.keybindings_changed && !outcome.layout_changed);
    assert_silent(&mut tables, "KeybindingsChanged").await;
    assert_silent(&mut layouts, "ApplyLayout").await;

    config.write("[mode.main.binding]\nalt-j = 'focus left'\n[gaps]\nouter.left = 20\n");
    let outcome = shared.reload_config(&daemon).await.unwrap();
    assert!(outcome.keybindings_changed && outcome.layout_changed);
    assert!(outcome.resend_keybindings && !outcome.builtin);
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert_eq!(table, vec![(0, "<Alt>j".to_owned())]);
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![(11, 20, 32, 950, 1048), (22, 970, 32, 950, 1048)]
    );

    // Binding id 0 now means the new binding.
    bridge(&shell, "KeybindingTriggered", &(0u32,))
        .await
        .unwrap();
    let (req, id): (u32, u64) = next_body(&mut focus).await;
    assert_eq!(id, 11);
    ack(&shell, req).await;

    // A broken file keeps the running settings.
    config.write("default-root-container-layout = 'spiral'\n");
    let err = shared.reload_config(&daemon).await.unwrap_err();
    assert!(format!("{err:?}").contains("Failed"), "{err:?}");
    assert_eq!(shared.state().keybindings.len(), 1);

    // A deleted file means the built-in config.
    std::fs::remove_file(config.path()).unwrap();
    let outcome = shared.reload_config(&daemon).await.unwrap();
    assert!(outcome.builtin && outcome.keybindings_changed && outcome.layout_changed);
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert_eq!(table.len(), 27);
}

#[tokio::test]
async fn reload_before_the_first_snapshot_sends_nothing() {
    let bus = Bus::start().await;
    let config = TempConfig::new("reload-early");
    config.write("[mode.main.binding]\nalt-h = 'focus left'\n");
    let (daemon, shared) = daemon_with(&bus, &config).await;
    let shell = bus.shell().await;
    let mut tables = bridge_signals(&shell, "KeybindingsChanged").await;
    hello(&shell).await;

    config.write("[mode.main.binding]\nalt-k = 'focus up'\n");
    let outcome = shared.reload_config(&daemon).await.unwrap();
    assert!(outcome.keybindings_changed && !outcome.resend_keybindings);
    assert_silent(&mut tables, "KeybindingsChanged before SyncWindows").await;

    // The first snapshot carries the reloaded table.
    sync_monitors(&shell).await;
    sync_windows(&shell, 0, vec![]).await;
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert_eq!(table, vec![(0, "<Alt>k".to_owned())]);
}

#[tokio::test]
async fn reload_needs_a_config_path() {
    let bus = Bus::start().await;
    let shared = Arc::new(Shared::new(Options::default()));
    let daemon = service::serve_shared(
        zbus::connection::Builder::address(bus.address.as_str()),
        shared.clone(),
    )
    .await
    .unwrap();
    assert!(shared.reload_config(&daemon).await.is_err());
}
