// SPDX-License-Identifier: MIT OR Apache-2.0
//! Integration tests against a private `dbus-daemon`.
//!
//! A mock "GNOME Shell" connection owns `org.gnome.Shell.Introspect`, exactly
//! like the real Shell does, and plays the extension's role.

mod common;

use std::time::Duration;

use common::*;
use futures_util::StreamExt;
use liwin_daemon::{BRIDGE_INTERFACE, BUS_NAME, DAEMON_INTERFACE, OBJECT_PATH, Options, service};

#[tokio::test]
async fn daemon_owns_its_bus_name() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let client = bus.connect().await;
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    assert!(
        dbus.name_has_owner(BUS_NAME.try_into().unwrap())
            .await
            .unwrap()
    );

    let version: zbus::OwnedVariant = call(
        &client,
        "org.freedesktop.DBus.Properties",
        "Get",
        &(DAEMON_INTERFACE, "Version"),
    )
    .await
    .unwrap();
    assert_eq!(
        String::try_from(version).unwrap(),
        env!("CARGO_PKG_VERSION")
    );
}

#[tokio::test]
async fn second_daemon_fails_to_start() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let second = service::serve(
        zbus::connection::Builder::address(bus.address.as_str()),
        Options::default(),
    )
    .await;
    assert!(second.is_err());
}

#[tokio::test]
async fn bridge_rejects_non_shell_callers() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let _shell = bus.shell().await;
    let intruder = bus.connect().await;

    let err = call::<_, u32>(&intruder, BRIDGE_INTERFACE, "Hello", &(2u32,))
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.AccessDenied");

    let err = call::<_, ()>(&intruder, BRIDGE_INTERFACE, "WindowClosed", &(1u64,))
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.AccessDenied");
}

#[tokio::test]
async fn bridge_requires_hello_first() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let err = call::<_, ()>(&shell, BRIDGE_INTERFACE, "WindowClosed", &(1u64,))
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.AccessDenied");
}

#[tokio::test]
async fn synced_windows_are_listed() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;

    let err = run(&cli, &["focus", "left"]).await.unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.ShellNotConnected");

    hello_and_sync(&shell).await;
    let out = run(&cli, &["list-windows"]).await.unwrap();
    assert!(out.contains("11"), "{out}");
    assert!(out.contains("* 22"), "{out}");
}

#[tokio::test]
async fn debug_commands_are_off_by_default() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let cli = bus.connect().await;
    let err = run(&cli, &["debug", "move-window", "1", "0", "0", "10", "10"])
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.InvalidCommand");
}

#[tokio::test]
async fn move_window_round_trip_is_unicast_to_shell() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let eavesdropper = bus.connect().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;

    let mut shell_signals = bridge_signals(&shell, "MoveResizeWindow").await;
    let mut other_signals = bridge_signals(&eavesdropper, "MoveResizeWindow").await;

    let shell_side = {
        let shell = shell.clone();
        tokio::spawn(async move {
            let msg = shell_signals.next().await.unwrap().unwrap();
            let (req, id, x, y, w, h): (u32, u64, i32, i32, i32, i32) =
                msg.body().deserialize().unwrap();
            assert_eq!((id, x, y, w, h), (11, 100, 120, 800, 600));
            let () = call(
                &shell,
                BRIDGE_INTERFACE,
                "RequestCompleted",
                &(req, true, ""),
            )
            .await
            .unwrap();
        })
    };

    let out = run(
        &cli,
        &["debug", "move-window", "11", "100", "120", "800", "600"],
    )
    .await
    .unwrap();
    assert_eq!(out, "ok");
    shell_side.await.unwrap();

    let leaked = tokio::time::timeout(Duration::from_millis(200), other_signals.next()).await;
    assert!(leaked.is_err(), "command signal must not reach other peers");
}

#[tokio::test]
async fn unanswered_request_times_out() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;

    let err = run(&cli, &["debug", "move-window", "11", "0", "0", "10", "10"])
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.Timeout");
}

#[tokio::test]
async fn focus_left_asks_shell_to_focus_neighbour() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;

    let mut signals = bridge_signals(&shell, "FocusWindow").await;
    let shell_side = {
        let shell = shell.clone();
        tokio::spawn(async move {
            let msg = signals.next().await.unwrap().unwrap();
            let (req, id): (u32, u64) = msg.body().deserialize().unwrap();
            assert_eq!(id, 11);
            let () = call(
                &shell,
                BRIDGE_INTERFACE,
                "RequestCompleted",
                &(req, true, ""),
            )
            .await
            .unwrap();
        })
    };
    run(&cli, &["focus", "left"]).await.unwrap();
    shell_side.await.unwrap();
}

#[tokio::test]
async fn goodbye_resets_state() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    let () = call(&shell, BRIDGE_INTERFACE, "Goodbye", &())
        .await
        .unwrap();
    let err = run(&cli, &["focus", "left"]).await.unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.ShellNotConnected");
    // A new Hello reconnects.
    hello_and_sync(&shell).await;
    assert!(run(&cli, &["list-windows"]).await.unwrap().contains("22"));
}

#[tokio::test]
async fn shell_disconnect_resets_state() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let cli = bus.connect().await;
    {
        let shell = bus.shell().await;
        hello_and_sync(&shell).await;
        shell.close().await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let err = run(&cli, &["focus", "left"]).await.unwrap_err();
    assert_eq!(error_name(&err), "io.github.liwin.Error.ShellNotConnected");
}

#[tokio::test]
async fn keybindings_are_sent_after_sync_and_trigger_commands() {
    let bus = Bus::start().await;
    let text = "[mode.main.binding]\nalt-h = 'focus left'\nalt-l = 'focus right'\n";
    let loaded = liwin_daemon::config::parse(text).unwrap();
    let options = Options {
        keybindings: loaded.config.main_bindings(&mut Vec::new()),
        ..Options::default()
    };
    let _daemon = bus.daemon(options).await;
    let shell = bus.shell().await;
    let mut tables = bridge_signals(&shell, "KeybindingsChanged").await;
    let mut focus = bridge_signals(&shell, "FocusWindow").await;

    hello_and_sync(&shell).await;
    let msg = tables.next().await.unwrap().unwrap();
    let (table,): (Vec<(u32, String)>,) = msg.body().deserialize().unwrap();
    assert_eq!(
        table,
        vec![(0, "<Alt>h".to_owned()), (1, "<Alt>l".to_owned())]
    );
    let () = call(
        &shell,
        BRIDGE_INTERFACE,
        "KeybindingsApplied",
        &(vec![1u32],),
    )
    .await
    .unwrap();

    // alt-h → focus left → FocusWindow(11)
    let () = call(&shell, BRIDGE_INTERFACE, "KeybindingTriggered", &(0u32,))
        .await
        .unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(2), focus.next())
        .await
        .expect("FocusWindow after keybinding")
        .unwrap()
        .unwrap();
    let (req, id): (u32, u64) = msg.body().deserialize().unwrap();
    assert_eq!(id, 11);
    let () = call(
        &shell,
        BRIDGE_INTERFACE,
        "RequestCompleted",
        &(req, true, ""),
    )
    .await
    .unwrap();

    // A second snapshot does not re-send the table.
    hello_and_sync(&shell).await;
    let again = tokio::time::timeout(Duration::from_millis(200), tables.next()).await;
    assert!(
        again.is_err(),
        "keybinding table must be sent once per Hello"
    );
}

#[tokio::test]
async fn snapshot_is_tiled_into_the_work_area() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await;
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![(11, 0, 32, 960, 1048), (22, 960, 32, 960, 1048)],
        "two windows side by side in the 1920x1048+0+32 work area"
    );
}

#[tokio::test]
async fn floating_windows_are_left_alone() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await;
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    let dialog: Wire = (33, 0, 0, 10, 10, 300, 200, 1);
    let () = call(&shell, BRIDGE_INTERFACE, "WindowOpened", &(dialog,))
        .await
        .unwrap();
    let nothing = tokio::time::timeout(Duration::from_millis(200), layouts.next()).await;
    assert!(
        nothing.is_err(),
        "a floating window does not change the layout"
    );
}

#[tokio::test]
async fn window_moved_to_other_workspace_retiles_both() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await;
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    let moved: Wire = (22, 1, 0, 960, 32, 960, 1048, 0);
    let () = call(&shell, BRIDGE_INTERFACE, "WindowChanged", &(moved,))
        .await
        .unwrap();
    let (mut batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    batch.sort();
    assert_eq!(
        batch,
        vec![(11, 0, 32, 1920, 1048), (22, 0, 32, 1920, 1048)]
    );
    let out = run(&cli, &["list-windows"]).await.unwrap();
    assert!(out.contains("22           workspace=2"), "{out}");
}

#[tokio::test]
async fn move_and_layout_commands_retile() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await; // h[11 *22]
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    run(&cli, &["move", "left"]).await.unwrap(); // h[*22 11]
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![(22, 0, 32, 960, 1048), (11, 960, 32, 960, 1048)]
    );

    run(&cli, &["layout", "tiles", "horizontal", "vertical"])
        .await
        .unwrap(); // v[*22 11]
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(batch, vec![(22, 0, 32, 1920, 524), (11, 0, 556, 1920, 524)]);

    // Setting the current orientation again is a no-op.
    run(&cli, &["layout", "vertical"]).await.unwrap();
    let nothing = tokio::time::timeout(Duration::from_millis(200), layouts.next()).await;
    assert!(nothing.is_err());
}

#[tokio::test]
async fn workspace_commands_reach_the_shell() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;

    let mut activate = bridge_signals(&shell, "ActivateWorkspace").await;
    let shell2 = shell.clone();
    let side = tokio::spawn(async move {
        let (req, ws): (u32, u32) = next_body(&mut activate).await;
        assert_eq!(ws, 2, "`workspace 3` is GNOME index 2");
        ack(&shell2, req).await;
    });
    run(&cli, &["workspace", "3"]).await.unwrap();
    side.await.unwrap();

    let mut move_ws = bridge_signals(&shell, "MoveWindowToWorkspace").await;
    let shell2 = shell.clone();
    let side = tokio::spawn(async move {
        let (req, id, ws): (u32, u64, u32) = next_body(&mut move_ws).await;
        assert_eq!((id, ws), (22, 0));
        ack(&shell2, req).await;
    });
    run(&cli, &["move-node-to-workspace", "1"]).await.unwrap();
    side.await.unwrap();
}

#[tokio::test]
async fn shell_events_are_applied_in_order() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;

    // Fire many events without waiting for replies, as the extension does;
    // the last one must win.
    for i in 0..200u64 {
        let id = if i % 2 == 0 { 11u64 } else { 0 };
        let msg = zbus::Message::method_call(OBJECT_PATH, "FocusChanged")
            .destination(BUS_NAME)
            .interface(BRIDGE_INTERFACE)
            .with_flags(zbus::message::Flags::NO_REPLY_EXPECTED)
            .build(&(id,))
            .unwrap();
        shell.send(&msg).await.unwrap();
    }
    let () = call(&shell, BRIDGE_INTERFACE, "FocusChanged", &(22u64,))
        .await
        .unwrap();
    let out = run(&cli, &["list-windows"]).await.unwrap();
    assert!(out.contains("* 22"), "{out}");
}
