// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers shared by the integration tests: a private `dbus-daemon`, a mock
//! GNOME Shell connection and small call/signal wrappers.
#![allow(dead_code)]

use std::process::Stdio;
use std::time::Duration;

use futures_util::StreamExt;
use liwin_daemon::{
    BRIDGE_INTERFACE, BUS_NAME, DAEMON_INTERFACE, OBJECT_PATH, Options, SHELL_IDENTITY_NAME,
    service,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use zbus::message::Type;
use zbus::{Connection, MatchRule, MessageStream};

/// `(id, workspace, monitor, x, y, width, height, flags)`
pub type Wire = (u64, u32, i32, i32, i32, i32, i32, u32);
pub type Placement = (u64, i32, i32, i32, i32);

pub struct Bus {
    _child: Child,
    pub address: String,
}

impl Bus {
    pub async fn start() -> Bus {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("dbus-daemon must be installed to run these tests");
        let stdout = child.stdout.take().unwrap();
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).await.unwrap();
        Bus {
            _child: child,
            address: line.trim().to_owned(),
        }
    }

    pub async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .build()
            .await
            .unwrap()
    }

    pub async fn daemon(&self, options: Options) -> Connection {
        service::serve(
            zbus::connection::Builder::address(self.address.as_str()),
            options,
        )
        .await
        .unwrap()
    }

    /// A connection posing as GNOME Shell.
    pub async fn shell(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .name(SHELL_IDENTITY_NAME)
            .build()
            .await
            .unwrap()
    }
}

pub fn debug_options() -> Options {
    Options {
        enable_debug_commands: true,
        request_timeout: Duration::from_millis(500),
        ..Options::default()
    }
}

pub async fn call<B, R>(conn: &Connection, iface: &str, method: &str, body: &B) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    let reply = conn
        .call_method(Some(BUS_NAME), OBJECT_PATH, Some(iface), method, body)
        .await?;
    reply.body().deserialize::<R>()
}

/// Calls a `ShellBridge1` method that returns nothing.
pub async fn bridge<B>(conn: &Connection, method: &str, body: &B) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    call::<_, ()>(conn, BRIDGE_INTERFACE, method, body).await
}

pub async fn run(conn: &Connection, args: &[&str]) -> zbus::Result<String> {
    call(conn, DAEMON_INTERFACE, "RunCommand", &(args,)).await
}

pub fn error_name(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        other => format!("{other:?}"),
    }
}

/// The error name of a failing `RunCommand`.
pub async fn run_err(conn: &Connection, args: &[&str]) -> String {
    match run(conn, args).await {
        Ok(out) => panic!("{args:?} unexpectedly succeeded: {out:?}"),
        Err(e) => error_name(&e),
    }
}

pub async fn hello(shell: &Connection) {
    let v: u32 = call(shell, BRIDGE_INTERFACE, "Hello", &(2u32,))
        .await
        .unwrap();
    assert_eq!(v, 2);
}

pub async fn sync_monitors(shell: &Connection) {
    let monitors: Vec<(i32, i32, i32, i32, i32)> = vec![(0, 0, 32, 1920, 1048)];
    bridge(shell, "SyncMonitors", &(monitors,)).await.unwrap();
}

pub async fn sync_windows(shell: &Connection, focused: u64, windows: Vec<Wire>) {
    bridge(shell, "SyncWindows", &(focused, windows))
        .await
        .unwrap();
}

/// Hello, one 1920x1048+0+32 monitor and windows 11 and 22 (22 focused):
/// the tree is `h[11 *22]`.
pub async fn hello_and_sync(shell: &Connection) {
    hello(shell).await;
    sync_monitors(shell).await;
    sync_windows(
        shell,
        22,
        vec![
            (11, 0, 0, 100, 100, 640, 480, 0),
            (22, 0, 0, 200, 200, 640, 480, 0),
        ],
    )
    .await;
}

pub async fn bridge_signals(conn: &Connection, member: &'static str) -> MessageStream {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(BUS_NAME)
        .interface(BRIDGE_INTERFACE)
        .member(member)
        .build()
        .unwrap();
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .unwrap()
}

pub async fn next_body<T>(stream: &mut MessageStream) -> T
where
    T: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    let msg = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("signal within 2s")
        .unwrap()
        .unwrap();
    msg.body().deserialize::<T>().unwrap()
}

/// Asserts that no message arrives on `stream` within 200 ms.
pub async fn assert_silent(stream: &mut MessageStream, what: &str) {
    let got = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;
    assert!(got.is_err(), "unexpected {what}: {got:?}");
}

pub async fn ack(shell: &Connection, req: u32) {
    bridge(shell, "RequestCompleted", &(req, true, ""))
        .await
        .unwrap();
}

pub async fn nack(shell: &Connection, req: u32, reason: &str) {
    bridge(shell, "RequestCompleted", &(req, false, reason))
        .await
        .unwrap();
}
