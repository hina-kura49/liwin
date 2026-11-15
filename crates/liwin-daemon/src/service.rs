//! Service wiring: shared state, the public `Daemon1` interface, and startup.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use futures_util::StreamExt;
use liwin_core::{Orientation, Rect, WindowId};
use tokio::sync::oneshot;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::names::{BusName, OwnedUniqueName};
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

use crate::bridge::{ShellBridge, ShellBridgeSignals};
use crate::commands::{self, Command};
use crate::model::Model;
use crate::{BUS_NAME, LiwinError, OBJECT_PATH, Options};

type RequestResult = Result<(), String>;

/// Mutable daemon state.
#[derive(Debug)]
pub struct State {
    /// Unique bus name of the authenticated GNOME Shell, once it said `Hello`.
    pub shell_peer: Option<OwnedUniqueName>,
    pub model: Model,
    /// Whether the keybinding table was sent to the current Shell peer.
    pub keybindings_sent: bool,
    next_request_id: u32,
    pending: HashMap<u32, oneshot::Sender<RequestResult>>,
}

impl State {
    /// Forgets the current Shell and fails all of its pending requests.
    pub fn disconnect_shell(&mut self) {
        self.shell_peer = None;
        self.keybindings_sent = false;
        self.model.clear();
        for (_, tx) in self.pending.drain() {
            let _ = tx.send(Err("shell disconnected".into()));
        }
    }

    pub fn complete_request(&mut self, id: u32, result: RequestResult) {
        if let Some(tx) = self.pending.remove(&id) {
            let _ = tx.send(result);
        }
    }
}

/// State shared by both interfaces and background tasks.
#[derive(Debug)]
pub struct Shared {
    pub options: Options,
    state: Mutex<State>,
}

impl Shared {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            state: Mutex::new(State {
                shell_peer: None,
                keybindings_sent: false,
                model: Model::new(Orientation::Horizontal),
                next_request_id: 1,
                pending: HashMap::new(),
            }),
        }
    }

    /// Locks the state. Never hold the guard across an `.await`.
    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn shell_peer(&self) -> Result<OwnedUniqueName, LiwinError> {
        self.state().shell_peer.clone().ok_or_else(|| {
            LiwinError::ShellNotConnected("the GNOME Shell extension is not connected".into())
        })
    }

    /// Emits a command signal to the Shell and waits for `RequestCompleted`.
    async fn request<F, Fut>(&self, conn: &Connection, emit: F) -> Result<(), LiwinError>
    where
        F: FnOnce(SignalEmitter<'static>, u32) -> Fut,
        Fut: Future<Output = zbus::Result<()>>,
    {
        let peer = self.shell_peer()?;
        let (id, rx) = {
            let mut st = self.state();
            let id = st.next_request_id;
            st.next_request_id = st.next_request_id.wrapping_add(1).max(1);
            let (tx, rx) = oneshot::channel();
            st.pending.insert(id, tx);
            (id, rx)
        };
        let emitter =
            SignalEmitter::new(conn, OBJECT_PATH)?.set_destination(BusName::Unique(peer.into()));
        if let Err(e) = emit(emitter, id).await {
            self.state().pending.remove(&id);
            return Err(e.into());
        }
        match tokio::time::timeout(self.options.request_timeout, rx).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(reason))) => Err(LiwinError::Failed(reason)),
            Ok(Err(_)) => Err(LiwinError::Failed("request dropped".into())),
            Err(_) => {
                self.state().pending.remove(&id);
                Err(LiwinError::Timeout(format!(
                    "the extension did not answer request {id} within {:?}",
                    self.options.request_timeout
                )))
            }
        }
    }

    pub async fn focus_window(
        &self,
        conn: &Connection,
        window: WindowId,
    ) -> Result<(), LiwinError> {
        self.request(
            conn,
            |e, id| async move { e.focus_window(id, window).await },
        )
        .await
    }

    pub async fn move_resize_window(
        &self,
        conn: &Connection,
        window: WindowId,
        r: Rect,
    ) -> Result<(), LiwinError> {
        self.request(conn, |e, id| async move {
            e.move_resize_window(id, window, r.x, r.y, r.width, r.height)
                .await
        })
        .await
    }

    /// Runs one command and returns its textual output.
    pub async fn run(&self, conn: &Connection, command: Command) -> Result<String, LiwinError> {
        if command.is_debug() && !self.options.enable_debug_commands {
            return Err(LiwinError::InvalidCommand(
                "debug commands are disabled (start liwin-daemon with --enable-debug-commands)"
                    .into(),
            ));
        }
        match command {
            Command::Help => Ok(commands::USAGE.to_owned()),
            Command::ListWindows => Ok(self.list_windows()),
            Command::DebugTree => Ok(self.dump_trees()),
            Command::DebugResync => {
                let peer = self.shell_peer()?;
                SignalEmitter::new(conn, OBJECT_PATH)?
                    .set_destination(BusName::Unique(peer.into()))
                    .resync_requested()
                    .await?;
                Ok(String::new())
            }
            Command::Focus(direction) => {
                self.shell_peer()?;
                let target = {
                    let mut st = self.state();
                    let Some(tree) = st.model.focused_tree_mut() else {
                        return Ok(String::new());
                    };
                    let moved = tree
                        .focus(direction)
                        .map_err(|e| LiwinError::Failed(e.to_string()))?;
                    if !moved {
                        return Ok(String::new());
                    }
                    tree.focused()
                };
                if let Some(target) = target {
                    self.focus_window(conn, target).await?;
                }
                Ok(String::new())
            }
            Command::DebugMoveWindow { window, rect } => {
                self.shell_peer()?;
                if !self.state().model.windows.contains_key(&window) {
                    return Err(LiwinError::Failed(format!("unknown window {window}")));
                }
                self.move_resize_window(conn, window, rect).await?;
                Ok("ok".into())
            }
        }
    }

    fn list_windows(&self) -> String {
        let st = self.state();
        let m = &st.model;
        m.windows
            .values()
            .map(|w| {
                let mark = if m.focused == Some(w.id) { "*" } else { " " };
                format!(
                    "{mark} {:<12} workspace={} monitor={} {}x{}+{}+{}",
                    w.id, w.workspace, w.monitor, w.rect.width, w.rect.height, w.rect.x, w.rect.y
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn dump_trees(&self) -> String {
        let st = self.state();
        st.model
            .trees
            .iter()
            .map(|(ws, t)| format!("workspace {ws}: {}", t.dump()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `io.github.liwin.Daemon1`: the public interface used by the CLI.
pub struct Daemon1 {
    shared: Arc<Shared>,
}

#[interface(name = "io.github.liwin.Daemon1")]
impl Daemon1 {
    /// Executes one command, e.g. `["focus", "left"]`.
    async fn run_command(
        &self,
        args: Vec<String>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<String, LiwinError> {
        let command = commands::parse(&args).map_err(LiwinError::InvalidCommand)?;
        tracing::debug!(?command, "RunCommand");
        self.shared.run(conn, command).await
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn shell_connected(&self) -> bool {
        self.shared.state().shell_peer.is_some()
    }
}

/// Starts the service on the connection described by `builder`.
///
/// Fails if `io.github.liwin.Daemon` is already owned (another daemon runs).
pub async fn serve(
    builder: zbus::connection::Builder<'_>,
    options: Options,
) -> anyhow::Result<Connection> {
    let shared = Arc::new(Shared::new(options));
    let conn = builder
        .serve_at(
            OBJECT_PATH,
            Daemon1 {
                shared: shared.clone(),
            },
        )
        .serve_at(OBJECT_PATH, ShellBridge::new(shared.clone()))
        .build()
        .await?;

    // `Builder::name` does not fail when the name is taken (it returns
    // `Exists`), so request it explicitly and insist on primary ownership.
    let reply = conn
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DO_NOT_QUEUE)
        .await?;
    if reply != RequestNameReply::PrimaryOwner {
        anyhow::bail!("{BUS_NAME} is already owned by another process ({reply:?})");
    }

    tokio::spawn(watch_shell_peer(conn.clone(), shared));

    // An extension that is already running answers with a full snapshot.
    SignalEmitter::new(&conn, OBJECT_PATH)?
        .resync_requested()
        .await?;
    Ok(conn)
}

/// Drops the Shell peer when its connection goes away (Shell restart,
/// crash). A disabled extension says `Goodbye` instead, because the Shell's
/// connection outlives it.
async fn watch_shell_peer(conn: Connection, shared: Arc<Shared>) {
    let proxy = match zbus::fdo::DBusProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("cannot watch bus names: {e}");
            return;
        }
    };
    let mut changes = match proxy.receive_name_owner_changed().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("cannot watch bus names: {e}");
            return;
        }
    };
    while let Some(signal) = changes.next().await {
        let Ok(args) = signal.args() else { continue };
        if args.new_owner().is_some() {
            continue;
        }
        let mut st = shared.state();
        let vanished = st
            .shell_peer
            .as_ref()
            .is_some_and(|peer| args.name().as_str() == peer.as_str());
        if vanished {
            tracing::info!("GNOME Shell peer {} vanished", args.name());
            st.disconnect_shell();
        }
    }
}
