// SPDX-License-Identifier: MIT OR Apache-2.0
//! Rendering of command output from the model.

use std::fmt::Write as _;

use crate::model::{ALL_WORKSPACES, Model, WindowInfo};

/// AeroSpace-style workspace name: GNOME index `0` is workspace `"1"`.
fn workspace_name(w: &WindowInfo) -> String {
    if w.workspace == ALL_WORKSPACES {
        "all".to_owned()
    } else {
        (u64::from(w.workspace) + 1).to_string()
    }
}

/// `list-windows`: one line per window, `*` marks the focused one.
pub fn list_windows(m: &Model) -> String {
    m.windows
        .values()
        .map(|w| {
            let mark = if m.focused == Some(w.id) { "*" } else { " " };
            let workspace = workspace_name(w);
            let floating = if w.floating { " floating" } else { "" };
            format!(
                "{mark} {:<12} workspace={workspace} monitor={} {}x{}+{}+{}{floating}",
                w.id, w.monitor, w.rect.width, w.rect.height, w.rect.x, w.rect.y
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `list-windows --json` (not exposed yet): a JSON array with one object per
/// window, using AeroSpace's format-variable names where one exists.
///
/// ```json
/// [{"window-id": 11, "workspace": "1", "monitor-id": 1,
///   "window-is-focused": true, "window-is-floating": false,
///   "x": 0, "y": 32, "width": 960, "height": 1048}]
/// ```
///
/// `workspace` is the AeroSpace-style name (`"1"`, ..., or `"all"`), and
/// `monitor-id` is 1-based like AeroSpace's (GNOME monitor index + 1).
/// Titles and application names are never known to the daemon.
pub fn list_windows_json(m: &Model) -> String {
    let mut out = String::from("[");
    for (i, w) in m.windows.values().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"window-id\":{},\"workspace\":{},\"monitor-id\":{},\
             \"window-is-focused\":{},\"window-is-floating\":{},\
             \"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
            w.id,
            json_string(&workspace_name(w)),
            i64::from(w.monitor) + 1,
            m.focused == Some(w.id),
            w.floating,
            w.rect.x,
            w.rect.y,
            w.rect.width,
            w.rect.height,
        );
    }
    out.push(']');
    out
}

/// `debug tree`: one line per tree.
pub fn dump_trees(m: &Model) -> String {
    m.trees
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

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use liwin_core::Rect;

    use super::*;
    use crate::model::{LayoutSettings, Monitor};

    fn model() -> Model {
        let mut m = Model::new(LayoutSettings::default());
        m.sync_monitors([Monitor {
            index: 0,
            work_area: Rect::new(0, 0, 1000, 500),
        }]);
        let win = |id, workspace, monitor, floating| WindowInfo {
            id,
            workspace,
            monitor,
            rect: Rect::new(1, 2, 3, 4),
            floating,
        };
        m.sync_windows(
            Some(2),
            [
                win(1, 0, 0, false),
                win(2, 0, 0, false),
                win(3, ALL_WORKSPACES, 1, true),
            ],
        );
        m.take_layout();
        m
    }

    #[test]
    fn text_list() {
        assert_eq!(
            list_windows(&model()),
            "  1            workspace=1 monitor=0 500x500+0+0\n\
             * 2            workspace=1 monitor=0 500x500+500+0\n\
             \x20 3            workspace=all monitor=1 3x4+1+2 floating"
        );
        assert_eq!(list_windows(&Model::new(LayoutSettings::default())), "");
    }

    #[test]
    fn json_list() {
        assert_eq!(
            list_windows_json(&model()),
            concat!(
                "[",
                r#"{"window-id":1,"workspace":"1","monitor-id":1,"window-is-focused":false,"window-is-floating":false,"x":0,"y":0,"width":500,"height":500},"#,
                r#"{"window-id":2,"workspace":"1","monitor-id":1,"window-is-focused":true,"window-is-floating":false,"x":500,"y":0,"width":500,"height":500},"#,
                r#"{"window-id":3,"workspace":"all","monitor-id":2,"window-is-focused":false,"window-is-floating":true,"x":1,"y":2,"width":3,"height":4}"#,
                "]"
            )
        );
        assert_eq!(
            list_windows_json(&Model::new(LayoutSettings::default())),
            "[]"
        );
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("a\"b\\c\n"), r#""a\"b\\c\u000a""#);
    }

    #[test]
    fn tree_dump() {
        assert_eq!(dump_trees(&model()), "workspace 1 monitor 0: h[1 *2]");
    }
}
