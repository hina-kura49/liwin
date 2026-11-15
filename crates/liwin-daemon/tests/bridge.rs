// SPDX-License-Identifier: MIT OR Apache-2.0
//! `ShellBridge1`: authorization, model reconciliation and keybindings.

mod common;

use std::time::Duration;

use common::*;
use liwin_daemon::{DAEMON_INTERFACE, Options, SHELL_IDENTITY_NAME};
use zbus::Connection;

const DENIED: &str = "io.github.liwin.Error.AccessDenied";

async fn shell_connected(conn: &Connection) -> bool {
    let v: zbus::OwnedVariant = call(
        conn,
        "org.freedesktop.DBus.Properties",
        "Get",
        &(DAEMON_INTERFACE, "ShellConnected"),
    )
    .await
    .unwrap();
    bool::try_from(v).unwrap()
}

/// Every `ShellBridge1` method except `Hello`, called by `conn`; returns
/// the error names.
async fn call_every_method(conn: &Connection) -> Vec<String> {
    let w: Wire = (11, 0, 0, 0, 0, 10, 10, 0);
    let results = [
        bridge(
            conn,
            "SyncMonitors",
            &(vec![(0i32, 0i32, 0i32, 10i32, 10i32)],),
        )
        .await,
        bridge(conn, "SyncWindows", &(0u64, Vec::<Wire>::new())).await,
        bridge(conn, "WindowOpened", &(w,)).await,
        bridge(conn, "WindowChanged", &(w,)).await,
        bridge(conn, "WindowClosed", &(11u64,)).await,
        bridge(conn, "FocusChanged", &(11u64,)).await,
        bridge(conn, "KeybindingTriggered", &(0u32,)).await,
        bridge(conn, "KeybindingsApplied", &(Vec::<u32>::new(),)).await,
        bridge(conn, "RequestCompleted", &(1u32, true, "")).await,
        bridge(conn, "Goodbye", &()).await,
    ];
    results
        .iter()
        .map(|r| r.as_ref().map_or_else(error_name, |()| "ok".into()))
        .collect()
}

#[tokio::test]
async fn only_the_hello_peer_may_call_the_bridge() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let intruder = bus.connect().await;
    let cli = bus.connect().await;

    // Before Hello, nobody is authorized, not even the Shell.
    assert_eq!(call_every_method(&shell).await, vec![DENIED; 10]);
    assert!(!shell_connected(&cli).await);

    hello_and_sync(&shell).await;
    assert!(shell_connected(&cli).await);
    assert_eq!(call_every_method(&intruder).await, vec![DENIED; 10]);
    // The intruder's Goodbye and WindowClosed had no effect.
    assert!(shell_connected(&cli).await);
    assert!(run(&cli, &["list-windows"]).await.unwrap().contains("11"));

    // An intruder cannot complete the Shell's pending requests.
    let mut moves = bridge_signals(&shell, "MoveResizeWindow").await;
    let side = {
        let shell = shell.clone();
        let intruder = intruder.clone();
        tokio::spawn(async move {
            let (req, ..): (u32, u64, i32, i32, i32, i32) = next_body(&mut moves).await;
            let err = bridge(&intruder, "RequestCompleted", &(req, false, "spoofed"))
                .await
                .unwrap_err();
            assert_eq!(error_name(&err), DENIED);
            ack(&shell, req).await;
        })
    };
    let out = run(&cli, &["debug", "move-window", "11", "0", "0", "10", "10"]).await;
    side.await.unwrap();
    assert_eq!(out.unwrap(), "ok");
}

#[tokio::test]
async fn hello_with_another_protocol_version_does_not_connect() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let v: u32 = call(&shell, liwin_daemon::BRIDGE_INTERFACE, "Hello", &(1u32,))
        .await
        .unwrap();
    assert_eq!(v, 2, "the daemon answers with its own version");
    assert!(!shell_connected(&cli).await);
    let err = bridge(&shell, "SyncWindows", &(0u64, Vec::<Wire>::new()))
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), DENIED);
}

#[tokio::test]
async fn hello_fails_when_nobody_owns_the_shell_name() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let not_shell = bus.connect().await;
    let err = call::<_, u32>(
        &not_shell,
        liwin_daemon::BRIDGE_INTERFACE,
        "Hello",
        &(2u32,),
    )
    .await
    .unwrap_err();
    assert_eq!(error_name(&err), DENIED);
}

#[tokio::test]
async fn repeated_hello_from_the_same_peer_keeps_the_model() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    hello(&shell).await;
    let out = run(&cli, &["list-windows"]).await.unwrap();
    assert!(out.contains("* 22"), "{out}");
}

#[tokio::test]
async fn a_new_shell_replaces_the_old_one() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let cli = bus.connect().await;
    let old = bus.shell().await;
    hello_and_sync(&old).await;

    // The old connection gives up the Shell's name; a new one takes it.
    old.release_name(SHELL_IDENTITY_NAME).await.unwrap();
    let new = bus.shell().await;
    hello(&new).await;
    assert_eq!(
        run(&cli, &["list-windows"]).await.unwrap(),
        "",
        "the old Shell's model is dropped"
    );
    let err = bridge(&old, "WindowClosed", &(11u64,)).await.unwrap_err();
    assert_eq!(error_name(&err), DENIED);
    // The old peer can no longer re-authenticate either.
    let err = call::<_, u32>(&old, liwin_daemon::BRIDGE_INTERFACE, "Hello", &(2u32,))
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), DENIED);
}

#[tokio::test]
async fn goodbye_fails_pending_requests() {
    let bus = Bus::start().await;
    let _daemon = bus
        .daemon(Options {
            request_timeout: Duration::from_secs(5),
            ..Options::default()
        })
        .await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    let mut activate = bridge_signals(&shell, "ActivateWorkspace").await;
    let side = {
        let shell = shell.clone();
        tokio::spawn(async move {
            let _: (u32, u32) = next_body(&mut activate).await;
            bridge(&shell, "Goodbye", &()).await.unwrap();
        })
    };
    let err = run(&cli, &["workspace", "2"]).await.unwrap_err();
    side.await.unwrap();
    assert_eq!(error_name(&err), "io.github.liwin.Error.Failed");
    assert!(format!("{err}").contains("shell disconnected"), "{err}");
    assert!(!shell_connected(&cli).await);
}

#[tokio::test]
async fn completing_an_unknown_request_is_ignored() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    hello_and_sync(&shell).await;
    ack(&shell, 12345).await;
    nack(&shell, 0, "whatever").await;
}

#[tokio::test]
async fn sync_windows_reconciles_the_model() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await; // h[11 *22]
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    // 11 is gone, 33 is new and focused, 22 keeps its place.
    sync_windows(
        &shell,
        33,
        vec![(22, 0, 0, 0, 0, 10, 10, 0), (33, 0, 0, 0, 0, 10, 10, 0)],
    )
    .await;
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![(22, 0, 32, 960, 1048), (33, 960, 32, 960, 1048)]
    );
    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: h[22 *33]"
    );

    // An identical snapshot still re-asserts the layout (undoes manual moves).
    sync_windows(
        &shell,
        33,
        vec![(22, 0, 0, 0, 0, 10, 10, 0), (33, 0, 0, 0, 0, 10, 10, 0)],
    )
    .await;
    let (again,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(again, batch);

    // An unknown focused id means no focus; a window that became floating
    // leaves its tree.
    sync_windows(
        &shell,
        99,
        vec![(22, 0, 0, 0, 0, 10, 10, 0), (33, 0, 0, 5, 5, 10, 10, 1)],
    )
    .await;
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(batch, vec![(22, 0, 32, 1920, 1048)]);
    assert_eq!(
        run(&cli, &["list-windows"]).await.unwrap(),
        "  22           workspace=1 monitor=0 1920x1048+0+32\n  33           workspace=1 monitor=0 10x10+5+5 floating"
    );

    // An empty snapshot drops everything.
    sync_windows(&shell, 0, vec![]).await;
    assert_silent(&mut layouts, "ApplyLayout").await;
    assert_eq!(run(&cli, &["debug", "tree"]).await.unwrap(), "");
}

#[tokio::test]
async fn window_events_update_the_model() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await; // h[11 *22]
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    // A new window goes next to the focused one.
    let w: Wire = (33, 0, 0, 0, 0, 10, 10, 0);
    bridge(&shell, "FocusChanged", &(11u64,)).await.unwrap();
    bridge(&shell, "WindowOpened", &(w,)).await.unwrap();
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![
            (11, 0, 32, 640, 1048),
            (33, 640, 32, 640, 1048),
            (22, 1280, 32, 640, 1048)
        ]
    );

    // WindowChanged to floating removes it from the tree.
    let floating: Wire = (33, 0, 0, 0, 0, 10, 10, 1);
    bridge(&shell, "WindowChanged", &(floating,)).await.unwrap();
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![(11, 0, 32, 960, 1048), (22, 960, 32, 960, 1048)]
    );

    // Closing re-tiles the rest; closing an unknown window is harmless.
    bridge(&shell, "WindowClosed", &(11u64,)).await.unwrap();
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(batch, vec![(22, 0, 32, 1920, 1048)]);
    bridge(&shell, "WindowClosed", &(999u64,)).await.unwrap();
    assert_silent(&mut layouts, "ApplyLayout").await;

    // The closed window had focus: nothing is focused now.
    assert!(!run(&cli, &["list-windows"]).await.unwrap().contains('*'));
}

#[tokio::test]
async fn monitor_changes_relayout_and_unknown_monitors_are_skipped() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await;
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    let monitors: Vec<(i32, i32, i32, i32, i32)> = vec![(0, 0, 0, 1000, 500)];
    bridge(&shell, "SyncMonitors", &(monitors,)).await.unwrap();
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(batch, vec![(11, 0, 0, 500, 500), (22, 500, 0, 500, 500)]);

    // A window on a monitor the daemon does not know is tracked, not placed.
    let w: Wire = (33, 0, 7, 0, 0, 10, 10, 0);
    bridge(&shell, "WindowOpened", &(w,)).await.unwrap();
    assert_silent(&mut layouts, "ApplyLayout").await;
}

#[tokio::test]
async fn the_keybinding_table_is_sent_once_per_hello() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let mut tables = bridge_signals(&shell, "KeybindingsChanged").await;

    // Even an empty table is sent after the first snapshot.
    hello(&shell).await;
    sync_monitors(&shell).await;
    assert_silent(&mut tables, "KeybindingsChanged before SyncWindows").await;
    sync_windows(&shell, 0, vec![]).await;
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert!(table.is_empty());

    // Goodbye + Hello starts over.
    bridge(&shell, "Goodbye", &()).await.unwrap();
    hello_and_sync(&shell).await;
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert!(table.is_empty());
}

fn options_with_bindings(text: &str) -> Options {
    let loaded = liwin_daemon::config::parse(text).unwrap();
    Options {
        keybindings: loaded.config.main_bindings(&mut Vec::new()),
        request_timeout: Duration::from_millis(500),
        ..Options::default()
    }
}

#[tokio::test]
async fn keybinding_commands_run_in_sequence_and_stop_on_failure() {
    let bus = Bus::start().await;
    let text = "[mode.main.binding]\n\
        alt-a = ['focus left', 'workspace 2']\n\
        alt-b = ['workspace 3', 'focus left']\n\
        alt-c = ['fly', 'focus left']\n";
    let _daemon = bus.daemon(options_with_bindings(text)).await;
    let shell = bus.shell().await;
    let mut tables = bridge_signals(&shell, "KeybindingsChanged").await;
    let mut focus = bridge_signals(&shell, "FocusWindow").await;
    let mut activate = bridge_signals(&shell, "ActivateWorkspace").await;
    hello_and_sync(&shell).await;
    let (table,): (Vec<(u32, String)>,) = next_body(&mut tables).await;
    assert_eq!(
        table,
        vec![
            (0, "<Alt>a".to_owned()),
            (1, "<Alt>b".to_owned()),
            (2, "<Alt>c".to_owned())
        ]
    );

    // alt-a: FocusWindow(11), then ActivateWorkspace(1) once acknowledged.
    bridge(&shell, "KeybindingTriggered", &(0u32,))
        .await
        .unwrap();
    let (req, id): (u32, u64) = next_body(&mut focus).await;
    assert_eq!(id, 11);
    assert_silent(&mut activate, "ActivateWorkspace before the ack").await;
    ack(&shell, req).await;
    let (req, ws): (u32, u32) = next_body(&mut activate).await;
    assert_eq!(ws, 1);
    ack(&shell, req).await;
    bridge(&shell, "FocusChanged", &(22u64,)).await.unwrap();

    // alt-b: the first command fails, the second never runs.
    bridge(&shell, "KeybindingTriggered", &(1u32,))
        .await
        .unwrap();
    let (req, ws): (u32, u32) = next_body(&mut activate).await;
    assert_eq!(ws, 2);
    nack(&shell, req, "no-such-workspace").await;
    assert_silent(&mut focus, "FocusWindow after a failed command").await;

    // alt-c: an unparsable command stops the chain too.
    bridge(&shell, "KeybindingTriggered", &(2u32,))
        .await
        .unwrap();
    assert_silent(&mut focus, "FocusWindow after an invalid command").await;

    // Unknown binding ids and rejected-id reports are harmless.
    bridge(&shell, "KeybindingTriggered", &(77u32,))
        .await
        .unwrap();
    bridge(&shell, "KeybindingsApplied", &(vec![0u32, 77],))
        .await
        .unwrap();
}
