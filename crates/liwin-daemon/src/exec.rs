// SPDX-License-Identifier: MIT OR Apache-2.0
//! Execution of [`Command`]s: one function per command.

use std::sync::Arc;

use liwin_core::{Direction, Rect, Tree, TreeError, WindowId};
use zbus::Connection;

use crate::LiwinError;
use crate::bridge::ShellBridgeSignals;
use crate::commands::{self, Command, LayoutChange};
use crate::config::KeyBinding;
use crate::model::ALL_WORKSPACES;
use crate::service::Shared;

/// Output of a command that prints nothing.
fn done() -> Result<String, LiwinError> {
    Ok(String::new())
}

fn tree_error(e: TreeError) -> LiwinError {
    LiwinError::Failed(e.to_string())
}

impl Shared {
    /// Runs one command and returns its textual output.
    pub async fn run(&self, conn: &Connection, command: Command) -> Result<String, LiwinError> {
        if command.is_debug() && !self.options.enable_debug_commands {
            return Err(LiwinError::InvalidCommand(
                "debug commands are disabled (start liwin-daemon with --enable-debug-commands)"
                    .into(),
            ));
        }
        if command.needs_shell() {
            self.shell_peer()?;
        }
        match command {
            Command::Help => Ok(commands::USAGE.to_owned()),
            Command::ListWindows => Ok(self.list_windows()),
            Command::DebugTree => Ok(self.dump_trees()),
            Command::DebugResync => self.debug_resync(conn).await,
            Command::Focus(direction) => self.focus(conn, direction).await,
            Command::Move(direction) => self.move_focused(conn, direction).await,
            Command::Layout(change) => self.layout(conn, change).await,
            Command::Workspace(index) => self.workspace(conn, index).await,
            Command::MoveNodeToWorkspace(index) => self.move_node_to_workspace(conn, index).await,
            Command::DebugMoveWindow { window, rect } => {
                self.debug_move_window(conn, window, rect).await
            }
        }
    }

    /// Runs the commands of a triggered keybinding in order, stopping at the
    /// first failure (which is logged).
    pub async fn run_binding(self: Arc<Self>, conn: Connection, binding: KeyBinding) {
        for args in &binding.commands {
            let result = match commands::parse(args) {
                Ok(command) => self.run(&conn, command).await,
                Err(e) => Err(LiwinError::InvalidCommand(e)),
            };
            if let Err(e) = result {
                tracing::warn!(key = %binding.key, command = %args.join(" "), "keybinding command failed: {e:?}");
                break;
            }
        }
    }

    /// Runs `f` on the tree of the focused window. `None` when no tiled
    /// window has focus.
    fn on_focused_tree<R>(
        &self,
        f: impl FnOnce(&mut Tree) -> Result<R, TreeError>,
    ) -> Result<Option<R>, LiwinError> {
        let mut st = self.state();
        let Some((_, tree)) = st.model.focused_tree_mut() else {
            return Ok(None);
        };
        f(tree).map(Some).map_err(tree_error)
    }

    /// Runs `f` on the tree of the focused window; if it reports a change,
    /// the tree is re-laid out and the layout sent to the Shell.
    async fn relayout_focused_tree(
        &self,
        conn: &Connection,
        f: impl FnOnce(&mut Tree) -> Result<bool, TreeError>,
    ) -> Result<String, LiwinError> {
        {
            let mut st = self.state();
            let Some((key, tree)) = st.model.focused_tree_mut() else {
                return done();
            };
            if f(tree).map_err(tree_error)? {
                st.model.mark_dirty(key);
            }
        }
        self.flush_layout(conn).await?;
        done()
    }

    async fn focus(&self, conn: &Connection, direction: Direction) -> Result<String, LiwinError> {
        let target = self
            .on_focused_tree(|tree| {
                Ok(if tree.focus(direction)? {
                    tree.focused()
                } else {
                    None
                })
            })?
            .flatten();
        if let Some(target) = target {
            self.focus_window(conn, target).await?;
        }
        done()
    }

    async fn move_focused(
        &self,
        conn: &Connection,
        direction: Direction,
    ) -> Result<String, LiwinError> {
        self.relayout_focused_tree(conn, |tree| tree.move_focused(direction))
            .await
    }

    async fn layout(&self, conn: &Connection, change: LayoutChange) -> Result<String, LiwinError> {
        self.relayout_focused_tree(conn, |tree| {
            let current = tree.focused_container_orientation()?;
            let wanted = match change {
                LayoutChange::Toggle => current.flipped(),
                LayoutChange::Set(o) => o,
            };
            if wanted == current {
                return Ok(false);
            }
            tree.toggle_orientation()?;
            Ok(true)
        })
        .await
    }

    async fn workspace(&self, conn: &Connection, index: u32) -> Result<String, LiwinError> {
        self.request(conn, |e, id| async move {
            e.activate_workspace(id, index).await
        })
        .await?;
        done()
    }

    async fn move_node_to_workspace(
        &self,
        conn: &Connection,
        index: u32,
    ) -> Result<String, LiwinError> {
        let Some(window) = self.state().model.focused else {
            return done();
        };
        self.request(conn, |e, id| async move {
            e.move_window_to_workspace(id, window, index).await
        })
        .await?;
        done()
    }

    async fn debug_move_window(
        &self,
        conn: &Connection,
        window: WindowId,
        rect: Rect,
    ) -> Result<String, LiwinError> {
        if !self.state().model.windows.contains_key(&window) {
            return Err(LiwinError::Failed(format!("unknown window {window}")));
        }
        self.move_resize_window(conn, window, rect).await?;
        Ok("ok".into())
    }

    async fn debug_resync(&self, conn: &Connection) -> Result<String, LiwinError> {
        self.shell_emitter(conn)?.resync_requested().await?;
        done()
    }

    fn list_windows(&self) -> String {
        let st = self.state();
        let m = &st.model;
        m.windows
            .values()
            .map(|w| {
                let mark = if m.focused == Some(w.id) { "*" } else { " " };
                let workspace = if w.workspace == ALL_WORKSPACES {
                    "all".to_owned()
                } else {
                    (w.workspace + 1).to_string()
                };
                let floating = if w.floating { " floating" } else { "" };
                format!(
                    "{mark} {:<12} workspace={workspace} monitor={} {}x{}+{}+{}{floating}",
                    w.id, w.monitor, w.rect.width, w.rect.height, w.rect.x, w.rect.y
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
            .map(|((ws, mon), t)| {
                format!(
                    "workspace {} monitor {mon}: {}",
                    ws.wrapping_add(1),
                    t.dump()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
