//! The daemon's view of the Shell: monitors, windows and per-workspace trees.
//!
//! The extension is the source of truth for which windows exist and where they
//! are; the trees are liwin's own structure on top of that.

use std::collections::BTreeMap;

use liwin_core::{Orientation, Rect, Tree, WindowId};

/// Wire form of a window: `(id, workspace, monitor, x, y, width, height)`.
pub type WireWindow = (u64, u32, i32, i32, i32, i32, i32);
/// Wire form of a monitor: `(index, x, y, width, height)` of its work area.
pub type WireMonitor = (i32, i32, i32, i32, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub workspace: u32,
    pub monitor: i32,
    pub rect: Rect,
}

impl From<WireWindow> for WindowInfo {
    fn from((id, workspace, monitor, x, y, w, h): WireWindow) -> Self {
        Self {
            id,
            workspace,
            monitor,
            rect: Rect::new(x, y, w, h),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub index: i32,
    pub work_area: Rect,
}

impl From<WireMonitor> for Monitor {
    fn from((index, x, y, w, h): WireMonitor) -> Self {
        Self {
            index,
            work_area: Rect::new(x, y, w, h),
        }
    }
}

/// Everything the daemon knows about the connected Shell.
#[derive(Debug)]
pub struct Model {
    root_orientation: Orientation,
    pub monitors: Vec<Monitor>,
    pub windows: BTreeMap<WindowId, WindowInfo>,
    pub focused: Option<WindowId>,
    pub trees: BTreeMap<u32, Tree>,
}

impl Model {
    pub fn new(root_orientation: Orientation) -> Self {
        Self {
            root_orientation,
            monitors: Vec::new(),
            windows: BTreeMap::new(),
            focused: None,
            trees: BTreeMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.monitors.clear();
        self.windows.clear();
        self.focused = None;
        self.trees.clear();
    }

    pub fn sync_monitors(&mut self, monitors: impl IntoIterator<Item = Monitor>) {
        self.monitors = monitors.into_iter().collect();
    }

    /// Replaces the window set with a full snapshot from the extension.
    pub fn sync_windows(
        &mut self,
        focused: Option<WindowId>,
        windows: impl IntoIterator<Item = WindowInfo>,
    ) {
        self.windows.clear();
        self.trees.clear();
        self.focused = None;
        for w in windows {
            self.window_opened(w);
        }
        self.focus_changed(focused);
    }

    pub fn window_opened(&mut self, w: WindowInfo) {
        if let Some(old) = self.windows.insert(w.id, w) {
            // Re-announced window: drop it from its previous tree first.
            if let Some(tree) = self.trees.get_mut(&old.workspace) {
                let _ = tree.remove_window(old.id);
            }
        }
        let orientation = self.root_orientation;
        let tree = self
            .trees
            .entry(w.workspace)
            .or_insert_with(|| Tree::new(orientation));
        tree.add_window(w.id)
            .expect("window was removed from its tree above");
    }

    pub fn window_closed(&mut self, id: WindowId) {
        if let Some(w) = self.windows.remove(&id)
            && let Some(tree) = self.trees.get_mut(&w.workspace)
        {
            let _ = tree.remove_window(id);
        }
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// Records the Shell's focus. Unknown windows (e.g. dialogs liwin does not
    /// manage) clear the focus.
    pub fn focus_changed(&mut self, id: Option<WindowId>) {
        self.focused = id.filter(|id| self.windows.contains_key(id));
        if let Some(w) = self.focused.and_then(|id| self.windows.get(&id))
            && let Some(tree) = self.trees.get_mut(&w.workspace)
        {
            let _ = tree.focus_window(w.id);
        }
    }

    /// The tree of the workspace holding the focused window.
    pub fn focused_tree_mut(&mut self) -> Option<&mut Tree> {
        let ws = self.windows.get(&self.focused?)?.workspace;
        self.trees.get_mut(&ws)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: u64, ws: u32) -> WindowInfo {
        WindowInfo {
            id,
            workspace: ws,
            monitor: 0,
            rect: Rect::new(0, 0, 10, 10),
        }
    }

    #[test]
    fn sync_builds_one_tree_per_workspace() {
        let mut m = Model::new(Orientation::Horizontal);
        m.sync_windows(Some(2), [win(1, 0), win(2, 0), win(3, 1)]);
        assert_eq!(m.trees[&0].dump(), "h[1 *2]");
        assert_eq!(m.trees[&1].dump(), "h[*3]");
        assert_eq!(m.focused, Some(2));
    }

    #[test]
    fn focus_on_unknown_window_clears_focus() {
        let mut m = Model::new(Orientation::Horizontal);
        m.sync_windows(Some(1), [win(1, 0)]);
        m.focus_changed(Some(99));
        assert_eq!(m.focused, None);
    }

    #[test]
    fn reopened_window_moves_between_workspaces() {
        let mut m = Model::new(Orientation::Horizontal);
        m.window_opened(win(1, 0));
        m.window_opened(win(1, 1));
        assert!(m.trees[&0].is_empty());
        assert!(m.trees[&1].contains(1));
    }

    #[test]
    fn close_removes_window() {
        let mut m = Model::new(Orientation::Horizontal);
        m.sync_windows(Some(1), [win(1, 0), win(2, 0)]);
        m.window_closed(1);
        assert_eq!(m.focused, None);
        assert_eq!(m.trees[&0].windows(), vec![2]);
    }
}
