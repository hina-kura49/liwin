//! Command language shared by the CLI and keybindings (AeroSpace-like).

use liwin_core::{Direction, Rect, WindowId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `focus left|right|up|down`
    Focus(Direction),
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

impl Command {
    pub fn is_debug(&self) -> bool {
        matches!(
            self,
            Command::DebugMoveWindow { .. } | Command::DebugTree | Command::DebugResync
        )
    }
}

pub const USAGE: &str = "\
usage:
  help
  focus left|right|up|down
  list-windows
  debug move-window <window-id> <x> <y> <width> <height>   (needs --enable-debug-commands)
  debug tree                                               (needs --enable-debug-commands)
  debug resync                                             (needs --enable-debug-commands)";

/// Parses one command from its argument vector.
pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Command, String> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    match args.as_slice() {
        ["focus", dir] => Ok(Command::Focus(dir.parse()?)),
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
    fn rejects_unknown() {
        assert!(parse::<&str>(&[]).is_err());
        assert!(parse(&["fly"]).is_err());
    }
}
