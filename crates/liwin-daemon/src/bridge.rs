//! `io.github.liwin.ShellBridge1`: the interface used by the GNOME Shell
//! extension. See `docs/dbus.md`.

use std::sync::Arc;

use zbus::message::Header;
use zbus::names::{BusName, OwnedUniqueName};
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

use crate::model::{WireMonitor, WireWindow};
use crate::service::Shared;
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
        let sender = hdr
            .sender()
            .ok_or_else(|| LiwinError::AccessDenied("message has no sender".into()))?;
        let st = self.shared.state();
        match &st.shell_peer {
            Some(peer) if peer.as_str() == sender.as_str() => Ok(peer.clone()),
            _ => Err(LiwinError::AccessDenied(
                "only the GNOME Shell peer that called Hello may use this interface".into(),
            )),
        }
    }
}

fn opt_window(id: u64) -> Option<u64> {
    (id != 0).then_some(id)
}

#[interface(name = "io.github.liwin.ShellBridge1")]
impl ShellBridge {
    /// Authenticates the caller as GNOME Shell: it must be the current owner of
    /// `org.gnome.Shell.Introspect`.
    async fn hello(
        &self,
        protocol_version: u32,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<u32, LiwinError> {
        let sender: OwnedUniqueName = hdr
            .sender()
            .ok_or_else(|| LiwinError::AccessDenied("message has no sender".into()))?
            .to_owned()
            .into();
        let shell_owner = zbus::fdo::DBusProxy::new(conn)
            .await?
            .get_name_owner(BusName::try_from(SHELL_IDENTITY_NAME)?)
            .await
            .map_err(|_| LiwinError::AccessDenied(format!("{SHELL_IDENTITY_NAME} has no owner")))?;
        if shell_owner.as_str() != sender.as_str() {
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
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        tracing::debug!(count = monitors.len(), "SyncMonitors");
        self.shared
            .state()
            .model
            .sync_monitors(monitors.into_iter().map(Into::into));
        Ok(())
    }

    async fn sync_windows(
        &self,
        focused: u64,
        windows: Vec<WireWindow>,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        tracing::info!(count = windows.len(), "SyncWindows");
        self.shared
            .state()
            .model
            .sync_windows(opt_window(focused), windows.into_iter().map(Into::into));
        Ok(())
    }

    async fn window_opened(
        &self,
        window: WireWindow,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        self.shared.state().model.window_opened(window.into());
        Ok(())
    }

    async fn window_closed(
        &self,
        window_id: u64,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        self.shared.state().model.window_closed(window_id);
        Ok(())
    }

    async fn focus_changed(
        &self,
        window_id: u64,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
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
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        tracing::debug!(
            binding_id,
            "KeybindingTriggered (keybindings not configured yet)"
        );
        Ok(())
    }

    async fn keybindings_applied(
        &self,
        rejected: Vec<u32>,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), LiwinError> {
        self.authorize(&hdr)?;
        tracing::info!(?rejected, "KeybindingsApplied");
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

    #[zbus(signal)]
    pub async fn keybindings_changed(
        emitter: &SignalEmitter<'_>,
        bindings: Vec<(u32, String)>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn resync_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
