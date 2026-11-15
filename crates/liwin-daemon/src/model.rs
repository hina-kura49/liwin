// SPDX-License-Identifier: MIT OR Apache-2.0
//! The daemon's view of the Shell: monitors, windows and the tiling trees.
//!
//! The extension is the source of truth for which windows exist and on which
//! workspace and monitor they are; the trees are liwin's own structure on top
//! of that. There is one tree per (workspace, monitor) pair, matching GNOME's
//! model where a workspace spans all monitors.

use std::collections::{BTreeMap, BTreeSet};

use liwin_core::{InnerGaps, Orientation, Rect, Tree, WindowId};

/// Workspace index meaning "on all workspaces" (e.g. windows on secondary
/// monitors when GNOME's `workspaces-only-on-primary` is set).
pub const ALL_WORKSPACES: u32 = u32::MAX;

/// The window must not be tiled (not resizable, transient, ...).
pub const FLAG_FLOATING: u32 = 1;

/// Wire form of a window:
/// `(id, workspace, monitor, x, y, width, height, flags)`.
pub type WireWindow = (u64, u32, i32, i32, i32, i32, i32, u32);
/// Wire form of a monitor: `(index, x, y, width, height)` of its work area.
pub type WireMonitor = (i32, i32, i32, i32, i32);

/// A tree is identified by (workspace, monitor).
pub type TreeKey = (u32, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub workspace: u32,
    pub monitor: i32,
    pub rect: Rect,
    pub floating: bool,
}

impl WindowInfo {
    /// The tree this window is tiled in, or `None` if it floats.
    pub fn tree_key(&self) -> Option<TreeKey> {
        (!self.floating).then_some((self.workspace, self.monitor))
    }
}

impl From<WireWindow> for WindowInfo {
    fn from((id, workspace, monitor, x, y, w, h, flags): WireWindow) -> Self {
        Self {
            id,
            workspace,
            monitor,
            rect: Rect::new(x, y, w, h),
            floating: flags & FLAG_FLOATING != 0,
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

/// How new trees are oriented (AeroSpace `default-root-container-orientation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootOrientation {
    Horizontal,
    Vertical,
    /// Horizontal on landscape monitors, vertical on portrait ones.
    #[default]
    Auto,
}

/// Outer gaps between the work area edges and the windows, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OuterGaps {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Layout parameters, from the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutSettings {
    pub root_orientation: RootOrientation,
    pub inner: InnerGaps,
    pub outer: OuterGaps,
}

/// Everything the daemon knows about the connected Shell.
#[derive(Debug)]
pub struct Model {
    settings: LayoutSettings,
    pub monitors: BTreeMap<i32, Monitor>,
    pub windows: BTreeMap<WindowId, WindowInfo>,
    pub focused: Option<WindowId>,
    pub trees: BTreeMap<TreeKey, Tree>,
    /// Trees whose layout must be (re)applied.
    dirty: BTreeSet<TreeKey>,
}

impl Model {
    pub fn new(settings: LayoutSettings) -> Self {
        Self {
            settings,
            monitors: BTreeMap::new(),
            windows: BTreeMap::new(),
            focused: None,
            trees: BTreeMap::new(),
            dirty: BTreeSet::new(),
        }
    }

    pub fn clear(&mut self) {
        self.monitors.clear();
        self.windows.clear();
        self.focused = None;
        self.trees.clear();
        self.dirty.clear();
    }

    pub fn sync_monitors(&mut self, monitors: impl IntoIterator<Item = Monitor>) {
        self.monitors = monitors.into_iter().map(|m| (m.index, m)).collect();
        self.dirty.extend(self.trees.keys().copied());
    }

    /// Reconciles the window set with a full snapshot from the extension.
    ///
    /// Windows keep their place in their tree; only windows that appeared,
    /// disappeared or changed workspace/monitor are touched. Every tree is
    /// then re-laid out, which also undoes manual moves.
    pub fn sync_windows(
        &mut self,
        focused: Option<WindowId>,
        windows: impl IntoIterator<Item = WindowInfo>,
    ) {
        let snapshot: Vec<WindowInfo> = windows.into_iter().collect();
        let present: BTreeSet<WindowId> = snapshot.iter().map(|w| w.id).collect();
        let gone: Vec<WindowId> = self
            .windows
            .keys()
            .filter(|id| !present.contains(id))
            .copied()
            .collect();
        for id in gone {
            self.window_closed(id);
        }
        for w in snapshot {
            self.window_opened(w);
        }
        self.dirty.extend(self.trees.keys().copied());
        self.focus_changed(focused);
    }

    /// A new window, or an update (workspace/monitor/floating) of a known one.
    pub fn window_opened(&mut self, w: WindowInfo) {
        let old_key = self.windows.insert(w.id, w).and_then(|old| old.tree_key());
        let new_key = w.tree_key();
        if old_key == new_key && old_key.is_some() {
            return;
        }
        if let Some(key) = old_key {
            self.remove_from_tree(key, w.id);
        }
        if let Some(key) = new_key {
            let orientation = self.orientation_for(key.1);
            let tree = self
                .trees
                .entry(key)
                .or_insert_with(|| Tree::new(orientation));
            tree.add_window(w.id)
                .expect("window was removed from its previous tree");
            self.dirty.insert(key);
        }
    }

    pub fn window_closed(&mut self, id: WindowId) {
        if let Some(key) = self.windows.remove(&id).and_then(|w| w.tree_key()) {
            self.remove_from_tree(key, id);
        }
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// Records the Shell's focus. Unknown windows (e.g. dialogs liwin does not
    /// manage) clear the focus.
    pub fn focus_changed(&mut self, id: Option<WindowId>) {
        self.focused = id.filter(|id| self.windows.contains_key(id));
        if let Some((key, id)) = self.focused_key()
            && let Some(tree) = self.trees.get_mut(&key)
        {
            let _ = tree.focus_window(id);
        }
    }

    /// The tree holding the focused window, if it is tiled.
    pub fn focused_tree_mut(&mut self) -> Option<(TreeKey, &mut Tree)> {
        let (key, _) = self.focused_key()?;
        self.trees.get_mut(&key).map(|t| (key, t))
    }

    pub fn mark_dirty(&mut self, key: TreeKey) {
        self.dirty.insert(key);
    }

    /// Computes the layout of every dirty tree and clears the dirty set.
    /// Window rectangles in the model are updated to the new layout.
    pub fn take_layout(&mut self) -> Vec<(WindowId, Rect)> {
        let mut out = Vec::new();
        for key in std::mem::take(&mut self.dirty) {
            let (Some(tree), Some(monitor)) = (self.trees.get(&key), self.monitors.get(&key.1))
            else {
                continue;
            };
            let o = self.settings.outer;
            let area = monitor.work_area.inset(o.left, o.top, o.right, o.bottom);
            for (id, rect) in tree.layout_with_gaps(area, self.settings.inner) {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.rect = rect;
                }
                out.push((id, rect));
            }
        }
        out
    }

    fn focused_key(&self) -> Option<(TreeKey, WindowId)> {
        let w = self.windows.get(&self.focused?)?;
        Some((w.tree_key()?, w.id))
    }

    fn remove_from_tree(&mut self, key: TreeKey, id: WindowId) {
        if let Some(tree) = self.trees.get_mut(&key) {
            let _ = tree.remove_window(id);
            if tree.is_empty() {
                self.trees.remove(&key);
            } else {
                self.dirty.insert(key);
            }
        }
    }

    fn orientation_for(&self, monitor: i32) -> Orientation {
        match self.settings.root_orientation {
            RootOrientation::Horizontal => Orientation::Horizontal,
            RootOrientation::Vertical => Orientation::Vertical,
            RootOrientation::Auto => match self.monitors.get(&monitor) {
                Some(m) if m.work_area.height > m.work_area.width => Orientation::Vertical,
                _ => Orientation::Horizontal,
            },
        }
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
            floating: false,
        }
    }

    fn model() -> Model {
        let mut m = Model::new(LayoutSettings::default());
        m.sync_monitors([Monitor {
            index: 0,
            work_area: Rect::new(0, 0, 1000, 500),
        }]);
        m
    }

    #[test]
    fn sync_builds_one_tree_per_workspace_and_monitor() {
        let mut m = model();
        m.sync_windows(Some(2), [win(1, 0), win(2, 0), win(3, 1)]);
        assert_eq!(m.trees[&(0, 0)].dump(), "h[1 *2]");
        assert_eq!(m.trees[&(1, 0)].dump(), "h[*3]");
        assert_eq!(m.focused, Some(2));
    }

    #[test]
    fn resync_keeps_tree_order() {
        let mut m = model();
        m.sync_windows(Some(2), [win(1, 0), win(2, 0)]);
        let (_, tree) = m.focused_tree_mut().unwrap();
        tree.move_focused(liwin_core::Direction::Left).unwrap();
        assert_eq!(m.trees[&(0, 0)].dump(), "h[*2 1]");

        // Same windows, Shell order, plus a new one and minus none.
        m.sync_windows(Some(2), [win(1, 0), win(2, 0), win(3, 0)]);
        assert_eq!(m.trees[&(0, 0)].windows(), vec![2, 3, 1]);

        // A window missing from the snapshot is gone.
        m.sync_windows(Some(2), [win(2, 0), win(3, 0)]);
        assert_eq!(m.trees[&(0, 0)].windows(), vec![2, 3]);
        assert!(!m.windows.contains_key(&1));
        assert_eq!(m.take_layout().len(), 2, "resync re-asserts the layout");
    }

    #[test]
    fn floating_windows_are_not_tiled() {
        let mut m = model();
        let mut dialog = win(2, 0);
        dialog.floating = true;
        m.sync_windows(Some(2), [win(1, 0), dialog]);
        assert_eq!(m.trees[&(0, 0)].dump(), "h[*1]");
        assert!(m.windows.contains_key(&2));
        assert!(m.focused_tree_mut().is_none(), "focused window floats");
    }

    #[test]
    fn focus_on_unknown_window_clears_focus() {
        let mut m = model();
        m.sync_windows(Some(1), [win(1, 0)]);
        m.focus_changed(Some(99));
        assert_eq!(m.focused, None);
    }

    #[test]
    fn workspace_change_moves_window_between_trees() {
        let mut m = model();
        m.window_opened(win(1, 0));
        m.window_opened(win(2, 0));
        m.take_layout();
        m.window_opened(win(1, 1));
        assert_eq!(m.trees[&(0, 0)].windows(), vec![2]);
        assert_eq!(m.trees[&(1, 0)].windows(), vec![1]);
        let layout = m.take_layout();
        assert_eq!(layout.len(), 2, "both trees are re-laid out: {layout:?}");
    }

    #[test]
    fn empty_trees_are_dropped() {
        let mut m = model();
        m.window_opened(win(1, 0));
        m.window_closed(1);
        assert!(m.trees.is_empty());
        assert!(m.take_layout().is_empty());
    }

    #[test]
    fn close_removes_window_and_relayouts() {
        let mut m = model();
        m.sync_windows(Some(1), [win(1, 0), win(2, 0)]);
        m.take_layout();
        m.window_closed(1);
        assert_eq!(m.focused, None);
        assert_eq!(m.take_layout(), vec![(2, Rect::new(0, 0, 1000, 500))]);
    }

    #[test]
    fn layout_applies_gaps() {
        let mut m = Model::new(LayoutSettings {
            root_orientation: RootOrientation::Auto,
            inner: InnerGaps {
                horizontal: 10,
                vertical: 0,
            },
            outer: OuterGaps {
                left: 5,
                top: 5,
                right: 5,
                bottom: 5,
            },
        });
        m.sync_monitors([Monitor {
            index: 0,
            work_area: Rect::new(0, 0, 1010, 510),
        }]);
        m.sync_windows(None, [win(1, 0), win(2, 0)]);
        assert_eq!(
            m.take_layout(),
            vec![
                (1, Rect::new(5, 5, 495, 500)),
                (2, Rect::new(510, 5, 495, 500)),
            ]
        );
        assert_eq!(m.windows[&1].rect, Rect::new(5, 5, 495, 500));
    }

    #[test]
    fn auto_orientation_follows_monitor_aspect() {
        let mut m = Model::new(LayoutSettings::default());
        m.sync_monitors([Monitor {
            index: 0,
            work_area: Rect::new(0, 0, 800, 1200),
        }]);
        m.window_opened(win(1, 0));
        assert_eq!(m.trees[&(0, 0)].root_orientation(), Orientation::Vertical);
    }
}
