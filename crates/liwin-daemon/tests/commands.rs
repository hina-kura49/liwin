// SPDX-License-Identifier: MIT OR Apache-2.0
//! `Daemon1.RunCommand`: execution of each command against a mock Shell.

mod common;

use common::*;
use liwin_daemon::Options;
use liwin_daemon::commands::USAGE;

const NOT_CONNECTED: &str = "io.github.liwin.Error.ShellNotConnected";
const INVALID: &str = "io.github.liwin.Error.InvalidCommand";
const FAILED: &str = "io.github.liwin.Error.Failed";
const TIMEOUT: &str = "io.github.liwin.Error.Timeout";

#[tokio::test]
async fn commands_without_a_shell() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let cli = bus.connect().await;

    for args in [
        &["focus", "left"][..],
        &["move", "right"],
        &["layout", "vertical"],
        &["layout", "tiles", "horizontal", "vertical"],
        &["workspace", "1"],
        &["move-node-to-workspace", "1"],
        &["debug", "move-window", "1", "0", "0", "10", "10"],
        &["debug", "resync"],
    ] {
        assert_eq!(run_err(&cli, args).await, NOT_CONNECTED, "{args:?}");
    }
    // Read-only commands work without a Shell.
    assert_eq!(run(&cli, &["list-windows"]).await.unwrap(), "");
    assert_eq!(run(&cli, &["debug", "tree"]).await.unwrap(), "");
    assert_eq!(run(&cli, &["help"]).await.unwrap(), USAGE);
}

#[tokio::test]
async fn invalid_commands_are_rejected() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let cli = bus.connect().await;
    for args in [
        &[][..],
        &["fly"],
        &["focus", "nowhere"],
        &["workspace", "0"],
        &["workspace", "web"],
        &["layout", "accordion"],
        &["list-windows", "--json"],
    ] {
        assert_eq!(run_err(&cli, args).await, INVALID, "{args:?}");
    }
}

#[tokio::test]
async fn debug_commands_are_gated_even_with_a_shell() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    for args in [
        &["debug", "tree"][..],
        &["debug", "resync"],
        &["debug", "move-window", "11", "0", "0", "10", "10"],
    ] {
        assert_eq!(run_err(&cli, args).await, INVALID, "{args:?}");
    }
}

#[tokio::test]
async fn focus_at_the_edge_is_a_no_op() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await; // h[11 *22]
    let mut focus = bridge_signals(&shell, "FocusWindow").await;

    for dir in ["right", "up", "down"] {
        assert_eq!(run(&cli, &["focus", dir]).await.unwrap(), "");
    }
    assert_silent(&mut focus, "FocusWindow").await;
    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: h[11 *22]"
    );
}

#[tokio::test]
async fn focus_moves_the_tree_focus_and_reports_failures() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    let mut focus = bridge_signals(&shell, "FocusWindow").await;

    let side = {
        let shell = shell.clone();
        tokio::spawn(async move {
            let (req, id): (u32, u64) = next_body(&mut focus).await;
            assert_eq!(id, 11);
            nack(&shell, req, "no-such-window").await;
        })
    };
    let err = run(&cli, &["focus", "left"]).await.unwrap_err();
    side.await.unwrap();
    assert_eq!(error_name(&err), FAILED);
    assert!(format!("{err}").contains("no-such-window"), "{err}");
    // The tree already moved its focus; the Shell's FocusChanged would
    // normally confirm it.
    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: h[*11 22]"
    );
}

#[tokio::test]
async fn focus_request_times_out() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    assert_eq!(run_err(&cli, &["focus", "left"]).await, TIMEOUT);
}

#[tokio::test]
async fn commands_without_a_focused_tiled_window_do_nothing() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    let mut focus = bridge_signals(&shell, "FocusWindow").await;
    let mut move_ws = bridge_signals(&shell, "MoveWindowToWorkspace").await;

    let check = async |cli: &zbus::Connection| {
        for args in [
            &["focus", "left"][..],
            &["move", "left"],
            &["layout", "vertical"],
            &["layout", "tiles", "horizontal", "vertical"],
        ] {
            assert_eq!(run(cli, args).await.unwrap(), "", "{args:?}");
        }
    };

    // No focus at all.
    bridge(&shell, "FocusChanged", &(0u64,)).await.unwrap();
    check(&cli).await;
    assert_eq!(
        run(&cli, &["move-node-to-workspace", "2"]).await.unwrap(),
        ""
    );
    assert_silent(&mut move_ws, "MoveWindowToWorkspace").await;

    // A floating window has focus.
    let dialog: Wire = (33, 0, 0, 10, 10, 300, 200, 1);
    bridge(&shell, "WindowOpened", &(dialog,)).await.unwrap();
    bridge(&shell, "FocusChanged", &(33u64,)).await.unwrap();
    check(&cli).await;

    assert_silent(&mut layouts, "ApplyLayout").await;
    assert_silent(&mut focus, "FocusWindow").await;
    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: h[11 *22]"
    );
}

#[tokio::test]
async fn move_at_the_edge_is_a_no_op() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await; // h[11 *22]
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    assert_eq!(run(&cli, &["move", "right"]).await.unwrap(), "");
    assert_silent(&mut layouts, "ApplyLayout").await;

    assert_eq!(run(&cli, &["move", "left"]).await.unwrap(), "");
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(
        batch,
        vec![(22, 0, 32, 960, 1048), (11, 960, 32, 960, 1048)]
    );
    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: h[*22 11]"
    );
}

#[tokio::test]
async fn layout_set_and_toggle() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    let mut layouts = bridge_signals(&shell, "ApplyLayout").await;
    hello_and_sync(&shell).await;
    let _: (Vec<Placement>,) = next_body(&mut layouts).await;

    // Already horizontal.
    assert_eq!(run(&cli, &["layout", "horizontal"]).await.unwrap(), "");
    assert_silent(&mut layouts, "ApplyLayout").await;

    assert_eq!(run(&cli, &["layout", "vertical"]).await.unwrap(), "");
    let (batch,): (Vec<Placement>,) = next_body(&mut layouts).await;
    assert_eq!(batch, vec![(11, 0, 32, 1920, 524), (22, 0, 556, 1920, 524)]);
    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: v[11 *22]"
    );

    // All four spellings of the toggle flip the orientation.
    for (args, expected) in [
        (&["layout", "tiles", "horizontal", "vertical"][..], "h"),
        (&["layout", "tiles", "vertical", "horizontal"], "v"),
        (&["layout", "horizontal", "vertical"], "h"),
        (&["layout", "vertical", "horizontal"], "v"),
    ] {
        assert_eq!(run(&cli, args).await.unwrap(), "");
        let _: (Vec<Placement>,) = next_body(&mut layouts).await;
        assert_eq!(
            run(&cli, &["debug", "tree"]).await.unwrap(),
            format!("workspace 1 monitor 0: {expected}[11 *22]"),
            "{args:?}"
        );
    }
}

#[tokio::test]
async fn workspace_maps_one_based_names_to_indices() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    let mut activate = bridge_signals(&shell, "ActivateWorkspace").await;

    for (name, index) in [("1", 0u32), ("9", 8), ("10", 9)] {
        let side = {
            let shell = shell.clone();
            let mut activate = activate;
            tokio::spawn(async move {
                let (req, ws): (u32, u32) = next_body(&mut activate).await;
                assert_eq!(ws, index, "workspace {name}");
                ack(&shell, req).await;
                activate
            })
        };
        assert_eq!(run(&cli, &["workspace", name]).await.unwrap(), "");
        activate = side.await.unwrap();
    }

    let side = {
        let shell = shell.clone();
        tokio::spawn(async move {
            let (req, _): (u32, u32) = next_body(&mut activate).await;
            nack(&shell, req, "no-such-workspace").await;
        })
    };
    let err = run(&cli, &["workspace", "42"]).await.unwrap_err();
    side.await.unwrap();
    assert_eq!(error_name(&err), FAILED);
    assert!(format!("{err}").contains("no-such-workspace"), "{err}");
}

#[tokio::test]
async fn move_node_to_workspace_moves_the_focused_window() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    bridge(&shell, "FocusChanged", &(11u64,)).await.unwrap();
    let mut move_ws = bridge_signals(&shell, "MoveWindowToWorkspace").await;

    let side = {
        let shell = shell.clone();
        tokio::spawn(async move {
            let (req, id, ws): (u32, u64, u32) = next_body(&mut move_ws).await;
            assert_eq!((id, ws), (11, 2));
            ack(&shell, req).await;
        })
    };
    assert_eq!(
        run(&cli, &["move-node-to-workspace", "3"]).await.unwrap(),
        ""
    );
    side.await.unwrap();
}

#[tokio::test]
async fn debug_commands() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(debug_options()).await;
    let shell = bus.shell().await;
    let eavesdropper = bus.connect().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    bridge(
        &shell,
        "WindowOpened",
        &((33u64, 1u32, 0i32, 0i32, 0i32, 10i32, 10i32, 0u32),),
    )
    .await
    .unwrap();

    assert_eq!(
        run(&cli, &["debug", "tree"]).await.unwrap(),
        "workspace 1 monitor 0: h[11 *22]\nworkspace 2 monitor 0: h[*33]"
    );

    let err = run(&cli, &["debug", "move-window", "99", "0", "0", "10", "10"])
        .await
        .unwrap_err();
    assert_eq!(error_name(&err), FAILED);
    assert!(format!("{err}").contains("unknown window 99"), "{err}");

    let mut resync = bridge_signals(&shell, "ResyncRequested").await;
    let mut leaked = bridge_signals(&eavesdropper, "ResyncRequested").await;
    assert_eq!(run(&cli, &["debug", "resync"]).await.unwrap(), "");
    let () = next_body(&mut resync).await;
    assert_silent(&mut leaked, "ResyncRequested on another peer").await;
}

#[tokio::test]
async fn list_windows_format() {
    let bus = Bus::start().await;
    let _daemon = bus.daemon(Options::default()).await;
    let shell = bus.shell().await;
    let cli = bus.connect().await;
    hello_and_sync(&shell).await;
    let dialog: Wire = (33, 0, 0, 10, 20, 300, 200, 1);
    let sticky: Wire = (44, u32::MAX, 1, 5, 6, 70, 80, 1);
    bridge(&shell, "WindowOpened", &(dialog,)).await.unwrap();
    bridge(&shell, "WindowOpened", &(sticky,)).await.unwrap();

    assert_eq!(
        run(&cli, &["list-windows"]).await.unwrap(),
        [
            "  11           workspace=1 monitor=0 960x1048+0+32",
            "* 22           workspace=1 monitor=0 960x1048+960+32",
            "  33           workspace=1 monitor=0 300x200+10+20 floating",
            "  44           workspace=all monitor=1 70x80+5+6 floating",
        ]
        .join("\n")
    );
}
