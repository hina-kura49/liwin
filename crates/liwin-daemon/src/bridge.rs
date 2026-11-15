// SPDX-License-Identifier: MIT OR Apache-2.0
//! `io.github.liwin.ShellBridge1`: the interface used by the GNOME Shell
//! extension. See `docs/dbus.md`.

use std::sync::Arc;

use zbus::message::Header;
use zbus::names::{BusName, OwnedUniqueName};
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

use crate::config::KeyBinding;
use crate::model::{Model, WireMonitor, WireWindow};
use crate::service::{Shared, unicast};
use crate::{LiwinError, PROTOCOL_VERSION, SHELL_IDENTITY_NAME};

pub struct ShellBridge {
    shared: Arc<Shared>,
}

impl ShellBridge {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }

    /// Accepts the call only from the Shell peer that completed `Hello`.
    fn authorize(&self, hdr: &Header<'_>) -> Result<OwnedUniqueName, LiwinError> {
        let sender = sender(hdr)?;
        let st = self.shared.state();
        match &st.shell_peer {
            Some(peer) if *peer == sender => Ok(peer.clone()),
            _ => Err(LiwinError::AccessDenied(
                "only the GNOME Shell peer that called Hello may use this interface".into(),
            )),
        }
    }

    /// Authorizes the call, applies `update` to the model and sends the
    /// resulting layout changes to the Shell.
    async fn update_model(
        &self,
        hdr: &Header<'_>,
        conn: &Connection,
        update: impl FnOnce(&mut Model),
    ) -> Result<(), LiwinError> {
        self.authorize(hdr)?;
        update(&mut self.shared.state().model);
        self.shared.flush_layout(conn).await
    }
}

fn sender(hdr: &Header<'_>) -> Result<OwnedUniqueName, LiwinError> {
    Ok(hdr
        .sender()
        .ok_or_else(|| LiwinError::AccessDenied("message has no sender".into()))?
        .to_owned()
        .into())
}

fn opt_window(id: u64) -> Option<u64> {
    (id != 0).then_some(id)
}

/// The `KeybindingsChanged` payload: binding ids are indices into `bindings`.
pub fn keybinding_table(bindings: &[KeyBinding]) -> Vec<(u32, String)> {
    bindings
        .iter()
        .enumerate()
        .map(|(i, b)| (i as u32, b.accelerator.clone()))
        .collect()
}

// `spawn = false`: zbus otherwise runs each call in its own task, which can
// reorder them (observed: WindowOpened/FocusChanged from the extension were
// applied out of order and focus was lost). Events from the Shell must be
// applied in the order they were sent.
#[interface(name = "io.github.liwin.ShellBridge1", spawn = false)]
impl ShellBridge {
    /// Authenticates the caller as GNOME Shell: it must be the current owner of
    /// `org.gnome.Shell.Introspect`.
    async fn hello(
        &self,
        protocol_version: u32,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<u32, LiwinError> {
        let sender = sender(&hdr)?;
        let shell_owner = zbus::fdo::DBusProxy::new(conn)
            .await?
            .get_name_owner(BusName::try_from(SHELL_IDENTITY_NAME)?)
            .await
            .map_err(|_| LiwinError::AccessDenied(format!("{SHELL_IDENTITY_NAME} has no owner")))?;
        if shell_owner != sender {
            tracing::warn!(%sender, "rejected Hello from a peer that is not GNOME Shell");
            return Err(LiwinError::AccessDenied(format!(
                "caller is not the owner of {SHELL_IDENTITY_NAME}"
            )));
        }
        if protocol_version != PROTOCOL_VERSION {
            tracing::error!(
                protocol_version,
                "extension speaks an unsupported protocol version"
            );
            return Ok(PROTOCOL_VERSION);
        }
        let mut st = self.shared.state();
        if st.shell_peer.as_ref() != Some(&sender) {
            st.disconnect_shell();
            st.shell_peer = Some(sender.clone());
        }
        tracing::info!(%sender, "GNOME Shell extension connected");
        Ok(PROTOCOL_VERSION)
    }

    /// The extension is being disabled: forget it until the next `Hello`.
    async fn goodbye(&self, #[zbus(header)] hdr: Header<'_>) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        tracing::info!("GNOME Shell extension disconnected (Goodbye)");
        self.shared.state().disconnect_shell();
        Ok(())
    }

    async fn sync_monitors(
        &self,
        monitors: Vec<WireMonitor>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), LiwinError> {
        tracing::debug!(count = monitors.len(), "SyncMonitors");
        self.update_model(&hdr, conn, |m| {
            m.sync_monitors(monitors.into_iter().map(Into::into))
        })
        .await
    }

    async fn sync_windows(
        &self,
        focused: u64,
        windows: Vec<WireWindow>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), LiwinError> {
        let peer = self.authorize(&hdr)?;
        tracing::info!(count = windows.len(), "SyncWindows");
        let send_keybindings = {
            let mut st = self.shared.state();
            st.model
                .sync_windows(opt_window(focused), windows.into_iter().map(Into::into));
            !std::mem::replace(&mut st.keybindings_sent, true)
        };
        // The first snapshot after Hello proves the extension is ready to
        // receive signals: send it the keybinding table (unicast).
        if send_keybindings {
            let table = keybinding_table(&self.shared.options.keybindings);
            unicast(conn, peer)?.keybindings_changed(table).await?;
        }
        self.shared.flush_layout(conn).await
    }

    async fn window_opened(
        &self,
        window: WireWindow,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), LiwinError> {
        self.update_model(&hdr, conn, |m| m.window_opened(window.into()))
            .await
    }

    /// A known window changed workspace, monitor or floating state.
    async fn window_changed(
        &self,
        window: WireWindow,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), LiwinError> {
        self.update_model(&hdr, conn, |m| m.window_opened(window.into()))
            .await
    }

    async fn window_closed(
        &self,
        window_id: u64,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), LiwinError> {
        self.update_model(&hdr, conn, |m| m.window_closed(window_id))
            .await
    }

    async fn focus_changed(
        &self,
        window_id: u64,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        tracing::debug!(window_id, "FocusChanged");
        self.shared
            .state()
            .model
            .focus_changed(opt_window(window_id));
        Ok(())
    }

    async fn keybinding_triggered(
        &self,
        binding_id: u32,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        let Some(binding) = self.shared.options.keybindings.get(binding_id as usize) else {
            tracing::warn!(binding_id, "unknown keybinding id");
            return Ok(());
        };
        tracing::debug!(key = %binding.key, "keybinding triggered");
        // Reply right away; the commands may wait for the extension, which
        // must not be blocked on this call.
        tokio::spawn(
            self.shared
                .clone()
                .run_binding(conn.clone(), binding.clone()),
        );
        Ok(())
    }

    async fn keybindings_applied(
        &self,
        rejected: Vec<u32>,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        let total = self.shared.options.keybindings.len();
        for &id in &rejected {
            if let Some(b) = self.shared.options.keybindings.get(id as usize) {
                tracing::warn!(
                    key = %b.key,
                    accelerator = %b.accelerator,
                    "keybinding rejected by GNOME Shell (already in use); not stealing it"
                );
            }
        }
        tracing::info!(
            grabbed = total.saturating_sub(rejected.len()),
            ?rejected,
            "KeybindingsApplied"
        );
        Ok(())
    }

    async fn request_completed(
        &self,
        request_id: u32,
        ok: bool,
        error: String,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        let result = if ok { Ok(()) } else { Err(error) };
        self.shared.state().complete_request(request_id, result);
        Ok(())
    }

    #[zbus(signal)]
    pub async fn move_resize_window(
        emitter: &SignalEmitter<'_>,
        request_id: u32,
        window_id: u64,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn focus_window(
        emitter: &SignalEmitter<'_>,
        request_id: u32,
        window_id: u64,
    ) -> zbus::Result<()>;

    /// Primitive batch move/resize from a layout pass. Not acknowledged.
    #[zbus(signal)]
    pub async fn apply_layout(
        emitter: &SignalEmitter<'_>,
        windows: Vec<(u64, i32, i32, i32, i32)>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn activate_workspace(
        emitter: &SignalEmitter<'_>,
        request_id: u32,
        workspace: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn move_window_to_workspace(
        emitter: &SignalEmitter<'_>,
        request_id: u32,
        window_id: u64,
        workspace: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn keybindings_changed(
        emitter: &SignalEmitter<'_>,
        bindings: Vec<(u32, String)>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn resync_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
