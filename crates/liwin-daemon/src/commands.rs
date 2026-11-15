// SPDX-License-Identifier: MIT OR Apache-2.0
//! Command language shared by the CLI and keybindings (AeroSpace-like).

use liwin_core::{Direction, Orientation, Rect, WindowId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `focus left|right|up|down`
    Focus(Direction),
    /// `move left|right|up|down`
    Move(Direction),
    /// `layout tiles horizontal vertical` (toggle) or `layout horizontal|vertical`
    Layout(LayoutChange),
    /// `workspace <n>`; `n` is 1-based like AeroSpace's default names, the
    /// payload is the 0-based GNOME workspace index.
    Workspace(u32),
    /// `move-node-to-workspace <n>` (payload: 0-based index)
    MoveNodeToWorkspace(u32),
    /// `list-windows`
    ListWindows,
    /// `debug move-window <id> <x> <y> <width> <height>`
    DebugMoveWindow { window: WindowId, rect: Rect },
    /// `debug tree`
    DebugTree,
    /// `debug resync`: ask the Shell for a fresh snapshot
    DebugResync,
    /// `help`
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutChange {
    Toggle,
    Set(Orientation),
}

impl Command {
    pub fn is_debug(&self) -> bool {
        matches!(
            self,
            Command::DebugMoveWindow { .. } | Command::DebugTree | Command::DebugResync
        )
    }

    /// Whether the command fails with `ShellNotConnected` when no Shell is
    /// connected. Read-only commands work without one.
    pub fn needs_shell(&self) -> bool {
        !matches!(
            self,
            Command::Help | Command::ListWindows | Command::DebugTree
        )
    }
}

pub const USAGE: &str = "\
usage:
  help
  focus left|right|up|down
  move left|right|up|down
  layout tiles horizontal vertical | layout horizontal | layout vertical
  workspace <1-based number>
  move-node-to-workspace <1-based number>
  list-windows
  debug move-window <window-id> <x> <y> <width> <height>   (needs --enable-debug-commands)
  debug tree                                               (needs --enable-debug-commands)
  debug resync                                             (needs --enable-debug-commands)";

/// Parses one command from its argument vector.
pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Command, String> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    match args.as_slice() {
        ["focus", dir] => Ok(Command::Focus(dir.parse()?)),
        ["move", dir] => Ok(Command::Move(dir.parse()?)),
        ["layout", "tiles", "horizontal", "vertical"]
        | ["layout", "tiles", "vertical", "horizontal"]
        | ["layout", "horizontal", "vertical"]
        | ["layout", "vertical", "horizontal"] => Ok(Command::Layout(LayoutChange::Toggle)),
        ["layout", "horizontal"] => Ok(Command::Layout(LayoutChange::Set(Orientation::Horizontal))),
        ["layout", "vertical"] => Ok(Command::Layout(LayoutChange::Set(Orientation::Vertical))),
        ["workspace", name] => Ok(Command::Workspace(workspace_index(name)?)),
        ["move-node-to-workspace", name] => {
            Ok(Command::MoveNodeToWorkspace(workspace_index(name)?))
        }
        ["list-windows"] => Ok(Command::ListWindows),
        ["help"] => Ok(Command::Help),
        ["debug", "move-window", id, x, y, w, h] => {
            let window = parse_num(id, "window-id")?;
            let rect = Rect::new(
                parse_num(x, "x")?,
                parse_num(y, "y")?,
                parse_num(w, "width")?,
                parse_num(h, "height")?,
            );
            if rect.width <= 0 || rect.height <= 0 {
                return Err("width and height must be positive".into());
            }
            Ok(Command::DebugMoveWindow { window, rect })
        }
        ["debug", "tree"] => Ok(Command::DebugTree),
        ["debug", "resync"] => Ok(Command::DebugResync),
        [] => Err(format!("no command given\n{USAGE}")),
        _ => Err(format!("unknown command '{}'\n{USAGE}", args.join(" "))),
    }
}

/// AeroSpace names workspaces; liwin maps numeric names `1`, `2`, ... onto
/// GNOME workspace indices `0`, `1`, ...
fn workspace_index(name: &str) -> Result<u32, String> {
    match name.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n - 1),
        _ => Err(format!(
            "unsupported workspace '{name}': only numeric workspaces 1, 2, ... are supported"
        )),
    }
}

fn parse_num<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("invalid {what}: '{s}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_focus() {
        assert_eq!(
            parse(&["focus", "left"]),
            Ok(Command::Focus(Direction::Left))
        );
        assert!(parse(&["focus", "nowhere"]).is_err());
        assert!(parse(&["focus"]).is_err());
    }

    #[test]
    fn parses_move_window() {
        assert_eq!(
            parse(&["debug", "move-window", "42", "-10", "20", "800", "600"]),
            Ok(Command::DebugMoveWindow {
                window: 42,
                rect: Rect::new(-10, 20, 800, 600)
            })
        );
        assert!(parse(&["debug", "move-window", "42", "0", "0", "0", "600"]).is_err());
        assert!(parse(&["debug", "move-window", "x", "0", "0", "1", "1"]).is_err());
    }

    #[test]
    fn parses_move_layout_and_workspaces() {
        assert_eq!(parse(&["move", "up"]), Ok(Command::Move(Direction::Up)));
        assert_eq!(
            parse(&["layout", "tiles", "horizontal", "vertical"]),
            Ok(Command::Layout(LayoutChange::Toggle))
        );
        assert_eq!(
            parse(&["layout", "vertical"]),
            Ok(Command::Layout(LayoutChange::Set(Orientation::Vertical)))
        );
        assert_eq!(parse(&["workspace", "1"]), Ok(Command::Workspace(0)));
        assert_eq!(
            parse(&["move-node-to-workspace", "9"]),
            Ok(Command::MoveNodeToWorkspace(8))
        );
        assert!(parse(&["workspace", "0"]).is_err());
        assert!(parse(&["workspace", "web"]).is_err());
        assert!(parse(&["layout", "accordion"]).is_err());
    }

    #[test]
    fn rejects_unknown() {
        assert!(parse::<&str>(&[]).is_err());
        assert!(parse(&["fly"]).is_err());
    }
}
