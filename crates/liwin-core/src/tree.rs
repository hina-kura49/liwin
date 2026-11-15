// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-workspace container tree.
//!
//! A workspace is a tree whose inner nodes are split containers (horizontal or
//! vertical) and whose leaves are windows. The root is always a container and
//! is never removed, even when empty.
//!
//! Invariants maintained after every public operation (checked by
//! [`Tree::check_invariants`]):
//! - every non-root container has at least two children (normalization),
//!   except a *pending split*: [`Tree::split`] wraps the focused window
//!   alone in a container so the next window lands next to it. A lone child
//!   is therefore always a window, never a container;
//! - the root never has a single child that is itself a container;
//! - parent links match child lists, every live node is reachable from the
//!   root exactly once, and every window is indexed;
//! - `focused` is `Some` iff the tree contains at least one window, and then
//!   points to one of its windows.

use std::collections::HashMap;
use std::fmt;

use crate::geometry::{Direction, Orientation, Rect};

/// Identifier of a window as given by the compositor (`Meta.Window.get_id()`).
pub type WindowId = u64;

/// Gaps between adjacent windows, in pixels (AeroSpace `gaps.inner`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InnerGaps {
    /// Between windows side by side.
    pub horizontal: i32,
    /// Between windows stacked vertically.
    pub vertical: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NodeId(usize);

#[derive(Debug)]
enum Kind {
    Container {
        orientation: Orientation,
        children: Vec<NodeId>,
    },
    Window(WindowId),
}

#[derive(Debug)]
struct Node {
    parent: Option<NodeId>,
    kind: Kind,
}

/// Errors returned by tree operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    DuplicateWindow(WindowId),
    NoSuchWindow(WindowId),
    NoFocusedWindow,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::DuplicateWindow(id) => write!(f, "window {id} is already in the tree"),
            TreeError::NoSuchWindow(id) => write!(f, "window {id} is not in the tree"),
            TreeError::NoFocusedWindow => write!(f, "no window is focused"),
        }
    }
}

impl std::error::Error for TreeError {}

/// A tree of split containers with windows as leaves.
#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Option<Node>>,
    free: Vec<usize>,
    root: NodeId,
    focused: Option<NodeId>,
    windows: HashMap<WindowId, NodeId>,
}

impl Tree {
    /// Creates an empty tree whose root container has `root_orientation`.
    pub fn new(root_orientation: Orientation) -> Self {
        let root = Node {
            parent: None,
            kind: Kind::Container {
                orientation: root_orientation,
                children: Vec::new(),
            },
        };
        Self {
            nodes: vec![Some(root)],
            free: Vec::new(),
            root: NodeId(0),
            focused: None,
            windows: HashMap::new(),
        }
    }

    /// Number of windows in the tree.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn contains(&self, window: WindowId) -> bool {
        self.windows.contains_key(&window)
    }

    /// The currently focused window, if any.
    pub fn focused(&self) -> Option<WindowId> {
        self.focused.map(|n| self.window_of(n))
    }

    pub fn root_orientation(&self) -> Orientation {
        self.orientation(self.root)
    }

    /// All windows in depth-first (visual) order.
    pub fn windows(&self) -> Vec<WindowId> {
        let mut out = Vec::with_capacity(self.windows.len());
        self.collect_windows(self.root, &mut out);
        out
    }

    /// Adds a window right after the focused window (in the focused window's
    /// container), or at the end of the root container if nothing is focused.
    /// The new window becomes focused.
    pub fn add_window(&mut self, window: WindowId) -> Result<(), TreeError> {
        if self.contains(window) {
            return Err(TreeError::DuplicateWindow(window));
        }
        let (parent, index) = match self.focused {
            Some(f) => {
                let (p, i) = self.position(f).expect("focused node has a parent");
                (p, i + 1)
            }
            None => (self.root, self.children(self.root).len()),
        };
        let node = self.alloc(Node {
            parent: Some(parent),
            kind: Kind::Window(window),
        });
        self.children_mut(parent).insert(index, node);
        self.windows.insert(window, node);
        self.focused = Some(node);
        Ok(())
    }

    /// Removes a window and normalizes the tree. If the removed window was
    /// focused, focus moves to the nearest window before it (or after it, if
    /// it was first) in its container, or in the closest ancestor where it has
    /// siblings.
    pub fn remove_window(&mut self, window: WindowId) -> Result<(), TreeError> {
        let node = self
            .windows
            .remove(&window)
            .ok_or(TreeError::NoSuchWindow(window))?;
        if self.focused == Some(node) {
            // Picked before detaching: windows survive normalization.
            self.focused = self.neighbor_leaf(node);
        }
        let (parent, index) = self.position(node).expect("window node has a parent");
        self.children_mut(parent).remove(index);
        self.release(node);
        self.normalize_upwards(parent);
        Ok(())
    }

    /// Focuses a specific window.
    pub fn focus_window(&mut self, window: WindowId) -> Result<(), TreeError> {
        let node = *self
            .windows
            .get(&window)
            .ok_or(TreeError::NoSuchWindow(window))?;
        self.focused = Some(node);
        Ok(())
    }

    /// Splits the focused window: it is wrapped into a new container with the
    /// given orientation, so the next window added lands next to it along that
    /// axis. If the focused window is the only child of its container, the
    /// container's orientation is changed instead (no useless nesting).
    pub fn split(&mut self, orientation: Orientation) -> Result<(), TreeError> {
        let focused = self.focused.ok_or(TreeError::NoFocusedWindow)?;
        let (parent, index) = self.position(focused).expect("focused node has a parent");
        if self.children(parent).len() == 1 {
            self.set_orientation(parent, orientation);
            return Ok(());
        }
        let container = self.alloc(Node {
            parent: Some(parent),
            kind: Kind::Container {
                orientation,
                children: vec![focused],
            },
        });
        self.children_mut(parent)[index] = container;
        self.node_mut(focused).parent = Some(container);
        Ok(())
    }

    /// Moves focus in `direction`, AeroSpace style: walk up from the focused
    /// window to the nearest ancestor laid out along the direction's axis that
    /// has a sibling in that direction, then descend into that sibling.
    ///
    /// Returns `Ok(false)` (and keeps focus) when there is nothing in that
    /// direction.
    pub fn focus(&mut self, direction: Direction) -> Result<bool, TreeError> {
        let mut current = self.focused.ok_or(TreeError::NoFocusedWindow)?;
        while let Some((parent, index)) = self.position(current) {
            if let Some(target) = self.sibling_index(parent, index, direction) {
                let sibling = self.children(parent)[target];
                self.focused = Some(self.descend(sibling, direction));
                return Ok(true);
            }
            current = parent;
        }
        Ok(false)
    }

    /// Computes the rectangle of every window when the root occupies `area`.
    /// Children of a container share its space equally.
    pub fn layout(&self, area: Rect) -> Vec<(WindowId, Rect)> {
        self.layout_with_gaps(area, InnerGaps::default())
    }

    /// Like [`Tree::layout`], with `gaps` between adjacent windows.
    pub fn layout_with_gaps(&self, area: Rect, gaps: InnerGaps) -> Vec<(WindowId, Rect)> {
        let mut out = Vec::with_capacity(self.windows.len());
        self.layout_node(self.root, area, gaps, &mut out);
        out
    }

    /// Orientation of the container holding the focused window.
    pub fn focused_container_orientation(&self) -> Result<Orientation, TreeError> {
        let focused = self.focused.ok_or(TreeError::NoFocusedWindow)?;
        let (parent, _) = self.position(focused).expect("focused node has a parent");
        Ok(self.orientation(parent))
    }

    /// Flips the orientation of the container holding the focused window
    /// (AeroSpace `layout tiles horizontal vertical`).
    pub fn toggle_orientation(&mut self) -> Result<Orientation, TreeError> {
        let focused = self.focused.ok_or(TreeError::NoFocusedWindow)?;
        let (parent, _) = self.position(focused).expect("focused node has a parent");
        let flipped = self.orientation(parent).flipped();
        self.set_orientation(parent, flipped);
        Ok(flipped)
    }

    /// Moves the focused window in `direction` (AeroSpace `move`):
    ///
    /// - next to a window along the container's axis: swap with it;
    /// - next to a container along the axis: move into that container, at
    ///   the side facing the window;
    /// - at the edge of its container: move out, next to the nearest
    ///   ancestor laid out along the axis;
    /// - no such ancestor: the root is re-oriented along the axis and the
    ///   window placed at that edge, the rest of the tree on the other side.
    ///
    /// A pending split around the window (see [`Tree::split`]) is dropped
    /// first, so the window moves relative to what is visible.
    ///
    /// The window keeps focus. Returns `Ok(false)` if nothing moved, in which
    /// case the tree is unchanged.
    pub fn move_focused(&mut self, direction: Direction) -> Result<bool, TreeError> {
        let focused = self.focused.ok_or(TreeError::NoFocusedWindow)?;
        let (parent, _) = self.position(focused).expect("focused node has a parent");
        let pending_split = (parent != self.root && self.children(parent).len() == 1)
            .then(|| self.orientation(parent));
        if pending_split.is_some() {
            self.normalize_upwards(parent);
        }
        let moved = self.move_window(focused, direction);
        if let (false, Some(orientation)) = (moved, pending_split) {
            self.split(orientation)?;
        }
        Ok(moved)
    }

    /// Moves window node `focused` as described in [`Tree::move_focused`].
    fn move_window(&mut self, focused: NodeId, direction: Direction) -> bool {
        let axis = direction.orientation();
        let forward = direction.is_forward();
        let (parent, index) = self.position(focused).expect("focused node has a parent");

        if let Some(target) = self.sibling_index(parent, index, direction) {
            let sibling = self.children(parent)[target];
            // A pending split (a lone window in a container) looks like a
            // window, so it is swapped with like one.
            if self.children(sibling).len() > 1 {
                self.children_mut(parent).remove(index);
                let at = if forward {
                    0
                } else {
                    self.children(sibling).len()
                };
                self.children_mut(sibling).insert(at, focused);
                self.node_mut(focused).parent = Some(sibling);
                self.normalize_upwards(parent);
            } else {
                self.children_mut(parent).swap(index, target);
            }
            return true;
        }

        // Move out of the current container.
        let mut branch = parent;
        while let Some((ancestor, branch_index)) = self.position(branch) {
            if self.orientation(ancestor) == axis {
                self.children_mut(parent).remove(index);
                let at = if forward {
                    branch_index + 1
                } else {
                    branch_index
                };
                self.children_mut(ancestor).insert(at, focused);
                self.node_mut(focused).parent = Some(ancestor);
                self.normalize_upwards(parent);
                return true;
            }
            branch = ancestor;
        }

        // No ancestor along the axis. If the root is laid out along it too,
        // the window is already at the edge.
        let root = self.root;
        if self.orientation(root) == axis || self.children(root).len() < 2 {
            return false;
        }
        self.children_mut(parent).remove(index);
        self.normalize_upwards(parent);
        let rest: Vec<NodeId> = std::mem::take(self.children_mut(root));
        let rest_node = if let [only] = rest.as_slice() {
            *only
        } else {
            let orientation = self.orientation(root);
            let c = self.alloc(Node {
                parent: Some(root),
                kind: Kind::Container {
                    orientation,
                    children: rest.clone(),
                },
            });
            for &n in &rest {
                self.node_mut(n).parent = Some(c);
            }
            c
        };
        self.node_mut(rest_node).parent = Some(root);
        self.node_mut(focused).parent = Some(root);
        let children = if forward {
            vec![rest_node, focused]
        } else {
            vec![focused, rest_node]
        };
        self.node_mut(root).kind = Kind::Container {
            orientation: axis,
            children,
        };
        true
    }

    /// Compact textual representation, mainly for tests and debugging.
    ///
    /// Containers are `h[...]` / `v[...]`, windows are their id, and the
    /// focused window is prefixed with `*`. Example: `h[1 v[*2 3]]`.
    pub fn dump(&self) -> String {
        let mut s = String::new();
        self.dump_node(self.root, &mut s);
        s
    }

    /// Verifies the structural invariants listed in the module documentation
    /// and returns a description of the first violation found.
    ///
    /// Meant for tests (including other crates' tests); it walks the whole
    /// arena, so it is not free.
    #[doc(hidden)]
    pub fn check_invariants(&self) -> Result<(), String> {
        let Some(root_node) = self.nodes.get(self.root.0).and_then(Option::as_ref) else {
            return Err("root slot is empty".into());
        };
        if root_node.parent.is_some() {
            return Err("root has a parent".into());
        }
        if !self.is_container(self.root) {
            return Err("root is not a container".into());
        }
        if let [only] = self.children(self.root)
            && self.is_container(*only)
        {
            return Err(format!(
                "root has a single container child: {}",
                self.dump()
            ));
        }

        let mut seen = vec![false; self.nodes.len()];
        let mut stack = vec![self.root];
        let mut window_count = 0;
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id.0).and_then(Option::as_ref) else {
                return Err(format!("dangling node id {}", id.0));
            };
            if std::mem::replace(&mut seen[id.0], true) {
                return Err(format!("node {} is reachable twice", id.0));
            }
            match &node.kind {
                Kind::Window(w) => {
                    window_count += 1;
                    if self.windows.get(w) != Some(&id) {
                        return Err(format!("window {w} is not indexed at node {}", id.0));
                    }
                }
                Kind::Container { children, .. } => {
                    if id != self.root {
                        match children.as_slice() {
                            [] => return Err(format!("empty non-root container {}", id.0)),
                            [only] if self.is_container(*only) => {
                                return Err(format!(
                                    "container {} wraps a lone container: {}",
                                    id.0,
                                    self.dump()
                                ));
                            }
                            _ => {}
                        }
                    }
                    for &c in children {
                        match self.nodes.get(c.0).and_then(Option::as_ref) {
                            Some(child) if child.parent == Some(id) => stack.push(c),
                            Some(_) => {
                                return Err(format!("node {} has a wrong parent link", c.0));
                            }
                            None => return Err(format!("dangling child id {}", c.0)),
                        }
                    }
                }
            }
        }
        if window_count != self.windows.len() {
            return Err(format!(
                "{window_count} windows reachable, {} indexed",
                self.windows.len()
            ));
        }
        for (slot, node) in self.nodes.iter().enumerate() {
            let free = self.free.contains(&slot);
            match (node.is_some(), free, seen[slot]) {
                (true, false, true) | (false, true, false) => {}
                _ => return Err(format!("slot {slot} leaked or double-freed")),
            }
        }
        match self.focused {
            None if !self.windows.is_empty() => Err("no focus in a non-empty tree".into()),
            None => Ok(()),
            Some(f) if !seen.get(f.0).copied().unwrap_or(false) => {
                Err(format!("focus points to unreachable node {}", f.0))
            }
            Some(f) if self.is_container(f) => Err("focus points to a container".into()),
            Some(_) => Ok(()),
        }
    }

    // ----- internals -------------------------------------------------------

    fn alloc(&mut self, node: Node) -> NodeId {
        if let Some(slot) = self.free.pop() {
            self.nodes[slot] = Some(node);
            NodeId(slot)
        } else {
            self.nodes.push(Some(node));
            NodeId(self.nodes.len() - 1)
        }
    }

    fn release(&mut self, id: NodeId) {
        debug_assert_ne!(id, self.root, "the root is never released");
        self.nodes[id.0] = None;
        self.free.push(id.0);
    }

    fn node(&self, id: NodeId) -> &Node {
        self.nodes[id.0].as_ref().expect("dangling node id")
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id.0].as_mut().expect("dangling node id")
    }

    fn children(&self, id: NodeId) -> &[NodeId] {
        match &self.node(id).kind {
            Kind::Container { children, .. } => children,
            Kind::Window(_) => &[],
        }
    }

    fn children_mut(&mut self, id: NodeId) -> &mut Vec<NodeId> {
        match &mut self.node_mut(id).kind {
            Kind::Container { children, .. } => children,
            Kind::Window(_) => panic!("windows have no children"),
        }
    }

    fn orientation(&self, id: NodeId) -> Orientation {
        match &self.node(id).kind {
            Kind::Container { orientation, .. } => *orientation,
            Kind::Window(_) => panic!("windows have no orientation"),
        }
    }

    fn set_orientation(&mut self, id: NodeId, value: Orientation) {
        if let Kind::Container { orientation, .. } = &mut self.node_mut(id).kind {
            *orientation = value;
        }
    }

    fn is_container(&self, id: NodeId) -> bool {
        matches!(self.node(id).kind, Kind::Container { .. })
    }

    fn window_of(&self, id: NodeId) -> WindowId {
        match self.node(id).kind {
            Kind::Window(w) => w,
            Kind::Container { .. } => panic!("not a window node"),
        }
    }

    /// Parent and index within the parent, or `None` for the root.
    fn position(&self, id: NodeId) -> Option<(NodeId, usize)> {
        let parent = self.node(id).parent?;
        let index = self
            .children(parent)
            .iter()
            .position(|&c| c == id)
            .expect("node is listed in its parent's children");
        Some((parent, index))
    }

    fn first_leaf(&self, mut id: NodeId) -> NodeId {
        while let Some(&first) = self.children(id).first() {
            id = first;
        }
        id
    }

    fn last_leaf(&self, mut id: NodeId) -> NodeId {
        while let Some(&last) = self.children(id).last() {
            id = last;
        }
        id
    }

    /// Index of the sibling of `parent`'s child `index` in `direction`, if
    /// `parent` is laid out along the direction's axis and has one.
    fn sibling_index(&self, parent: NodeId, index: usize, direction: Direction) -> Option<usize> {
        if self.orientation(parent) != direction.orientation() {
            return None;
        }
        if direction.is_forward() {
            (index + 1 < self.children(parent).len()).then_some(index + 1)
        } else {
            index.checked_sub(1)
        }
    }

    /// The window closest to `id` in tree order, looking first among its
    /// siblings and then among those of its ancestors: the last window of
    /// the previous sibling, or the first window of the next one. `None` if
    /// `id` holds the tree's only windows.
    fn neighbor_leaf(&self, mut id: NodeId) -> Option<NodeId> {
        while let Some((parent, index)) = self.position(id) {
            let siblings = self.children(parent);
            if index > 0 {
                return Some(self.last_leaf(siblings[index - 1]));
            }
            if let Some(&next) = siblings.get(1) {
                return Some(self.first_leaf(next));
            }
            id = parent;
        }
        None
    }

    /// Picks the leaf to focus when entering `id` while moving in `direction`:
    /// the child nearest to where we came from along the movement axis, the
    /// first child otherwise.
    fn descend(&self, mut id: NodeId, direction: Direction) -> NodeId {
        while self.is_container(id) {
            let children = self.children(id);
            let along_axis = self.orientation(id) == direction.orientation();
            id = if along_axis && !direction.is_forward() {
                *children.last().expect("non-root containers are not empty")
            } else {
                *children.first().expect("non-root containers are not empty")
            };
        }
        id
    }

    /// Restores the invariants starting at `start` and walking to the root.
    fn normalize_upwards(&mut self, start: NodeId) {
        let mut current = start;
        while let Some((parent, index)) = self.position(current) {
            match self.children(current).len() {
                0 => {
                    self.children_mut(parent).remove(index);
                    self.release(current);
                }
                1 => {
                    let only = self.children(current)[0];
                    self.children_mut(parent)[index] = only;
                    self.node_mut(only).parent = Some(parent);
                    self.release(current);
                }
                _ => return,
            }
            current = parent;
        }
        self.absorb_single_root_child();
    }

    /// If the root has exactly one child and it is a container, the root takes
    /// over that container's orientation and children.
    fn absorb_single_root_child(&mut self) {
        let root = self.root;
        let [only] = self.children(root) else { return };
        let only = *only;
        if !self.is_container(only) {
            return;
        }
        let orientation = self.orientation(only);
        let grandchildren = std::mem::take(self.children_mut(only));
        for &c in &grandchildren {
            self.node_mut(c).parent = Some(root);
        }
        self.release(only);
        self.node_mut(root).kind = Kind::Container {
            orientation,
            children: grandchildren,
        };
    }

    fn collect_windows(&self, id: NodeId, out: &mut Vec<WindowId>) {
        match &self.node(id).kind {
            Kind::Window(w) => out.push(*w),
            Kind::Container { children, .. } => {
                for &c in children {
                    self.collect_windows(c, out);
                }
            }
        }
    }

    fn layout_node(
        &self,
        id: NodeId,
        area: Rect,
        gaps: InnerGaps,
        out: &mut Vec<(WindowId, Rect)>,
    ) {
        match &self.node(id).kind {
            Kind::Window(w) => out.push((*w, area)),
            Kind::Container {
                orientation,
                children,
            } => {
                let gap = match orientation {
                    Orientation::Horizontal => gaps.horizontal,
                    Orientation::Vertical => gaps.vertical,
                };
                for (&child, rect) in
                    children
                        .iter()
                        .zip(area.split_with_gap(*orientation, children.len(), gap))
                {
                    self.layout_node(child, rect, gaps, out);
                }
            }
        }
    }

    fn dump_node(&self, id: NodeId, s: &mut String) {
        match &self.node(id).kind {
            Kind::Window(w) => {
                if self.focused == Some(id) {
                    s.push('*');
                }
                s.push_str(&w.to_string());
            }
            Kind::Container {
                orientation,
                children,
            } => {
                s.push(match orientation {
                    Orientation::Horizontal => 'h',
                    Orientation::Vertical => 'v',
                });
                s.push('[');
                for (i, &c) in children.iter().enumerate() {
                    if i > 0 {
                        s.push(' ');
                    }
                    self.dump_node(c, s);
                }
                s.push(']');
            }
        }
    }
}

#[cfg(test)]
mod proptests;

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::*;
    use Orientation::*;

    fn tree_with(windows: &[WindowId]) -> Tree {
        let mut t = Tree::new(Horizontal);
        for &w in windows {
            t.add_window(w).unwrap();
        }
        t
    }

    #[test]
    fn empty_tree() {
        let t = Tree::new(Horizontal);
        assert!(t.is_empty());
        assert_eq!(t.focused(), None);
        assert_eq!(t.dump(), "h[]");
        assert!(t.layout(Rect::new(0, 0, 100, 100)).is_empty());
    }

    #[test]
    fn add_appends_and_focuses_new_window() {
        let t = tree_with(&[1, 2, 3]);
        assert_eq!(t.dump(), "h[1 2 *3]");
        assert_eq!(t.focused(), Some(3));
        assert_eq!(t.windows(), vec![1, 2, 3]);
    }

    #[test]
    fn add_inserts_after_focused_window() {
        let mut t = tree_with(&[1, 2, 3]);
        t.focus_window(1).unwrap();
        t.add_window(4).unwrap();
        assert_eq!(t.dump(), "h[1 *4 2 3]");
    }

    #[test]
    fn add_duplicate_fails() {
        let mut t = tree_with(&[1]);
        assert_eq!(t.add_window(1), Err(TreeError::DuplicateWindow(1)));
    }

    #[test]
    fn remove_moves_focus_to_previous_sibling() {
        let mut t = tree_with(&[1, 2, 3]);
        t.focus_window(2).unwrap();
        t.remove_window(2).unwrap();
        assert_eq!(t.dump(), "h[*1 3]");
    }

    #[test]
    fn remove_first_moves_focus_to_next_sibling() {
        let mut t = tree_with(&[1, 2]);
        t.focus_window(1).unwrap();
        t.remove_window(1).unwrap();
        assert_eq!(t.dump(), "h[*2]");
    }

    #[test]
    fn remove_from_pending_split_focuses_neighbor() {
        // Found by property testing: focus used to jump to the first window
        // of the tree (0) instead of the removed window's neighbor (1).
        let mut t = tree_with(&[0, 1, 8]);
        t.split(Horizontal).unwrap();
        assert_eq!(t.dump(), "h[0 1 h[*8]]");
        t.remove_window(8).unwrap();
        assert_eq!(t.dump(), "h[0 *1]");

        let mut t = tree_with(&[1, 2, 3]);
        t.focus_window(1).unwrap();
        t.split(Vertical).unwrap();
        t.remove_window(1).unwrap();
        assert_eq!(t.dump(), "h[*2 3]");
    }

    #[test]
    fn remove_unfocused_keeps_focus() {
        let mut t = tree_with(&[1, 2, 3]);
        t.remove_window(1).unwrap();
        assert_eq!(t.dump(), "h[2 *3]");
    }

    #[test]
    fn remove_last_window_empties_tree() {
        let mut t = tree_with(&[1]);
        t.remove_window(1).unwrap();
        assert_eq!(t.dump(), "h[]");
        assert_eq!(t.focused(), None);
    }

    #[test]
    fn remove_unknown_fails() {
        let mut t = tree_with(&[1]);
        assert_eq!(t.remove_window(9), Err(TreeError::NoSuchWindow(9)));
    }

    #[test]
    fn split_wraps_focused_window() {
        let mut t = tree_with(&[1, 2]);
        t.split(Vertical).unwrap();
        assert_eq!(t.dump(), "h[1 v[*2]]");
        t.add_window(3).unwrap();
        assert_eq!(t.dump(), "h[1 v[2 *3]]");
    }

    #[test]
    fn split_single_child_changes_orientation() {
        let mut t = tree_with(&[1]);
        t.split(Vertical).unwrap();
        assert_eq!(t.dump(), "v[*1]");
        assert_eq!(t.root_orientation(), Vertical);
    }

    #[test]
    fn split_without_focus_fails() {
        let mut t = Tree::new(Horizontal);
        assert_eq!(t.split(Vertical), Err(TreeError::NoFocusedWindow));
    }

    #[test]
    fn remove_flattens_single_child_container() {
        let mut t = tree_with(&[1, 2]);
        t.split(Vertical).unwrap();
        t.add_window(3).unwrap();
        assert_eq!(t.dump(), "h[1 v[2 *3]]");
        t.remove_window(3).unwrap();
        assert_eq!(t.dump(), "h[1 *2]");
    }

    #[test]
    fn root_absorbs_lone_container() {
        let mut t = tree_with(&[1, 2]);
        t.split(Vertical).unwrap();
        t.add_window(3).unwrap();
        t.remove_window(1).unwrap();
        assert_eq!(t.dump(), "v[2 *3]");
    }

    /// Builds `h[1 v[2 3]]` with 1 focused.
    fn nested() -> Tree {
        let mut t = tree_with(&[1, 2]);
        t.split(Vertical).unwrap();
        t.add_window(3).unwrap();
        t.focus_window(1).unwrap();
        t
    }

    #[test]
    fn focus_horizontal_moves_between_siblings() {
        let mut t = tree_with(&[1, 2, 3]);
        assert_eq!(t.focus(Left), Ok(true));
        assert_eq!(t.focused(), Some(2));
        assert_eq!(t.focus(Left), Ok(true));
        assert_eq!(t.focused(), Some(1));
        assert_eq!(t.focus(Left), Ok(false));
        assert_eq!(t.focused(), Some(1));
        assert_eq!(t.focus(Right), Ok(true));
        assert_eq!(t.focused(), Some(2));
    }

    #[test]
    fn focus_across_axis_is_noop_in_flat_tree() {
        let mut t = tree_with(&[1, 2]);
        assert_eq!(t.focus(Up), Ok(false));
        assert_eq!(t.focus(Down), Ok(false));
        assert_eq!(t.focused(), Some(2));
    }

    #[test]
    fn focus_in_nested_tree() {
        let mut t = nested();
        assert_eq!(t.dump(), "h[*1 v[2 3]]");

        // Entering the vertical container from the left picks its first child.
        assert_eq!(t.focus(Right), Ok(true));
        assert_eq!(t.focused(), Some(2));
        // Moving down stays inside the vertical container.
        assert_eq!(t.focus(Down), Ok(true));
        assert_eq!(t.focused(), Some(3));
        assert_eq!(t.focus(Down), Ok(false));
        // Moving left from 3 walks up to the horizontal root.
        assert_eq!(t.focus(Left), Ok(true));
        assert_eq!(t.focused(), Some(1));
    }

    #[test]
    fn focus_entering_orthogonal_container_picks_first_child() {
        let mut t = tree_with(&[1, 2, 3]);
        t.focus_window(2).unwrap();
        t.split(Vertical).unwrap();
        t.add_window(4).unwrap();
        t.focus_window(3).unwrap();
        assert_eq!(t.dump(), "h[1 v[2 4] *3]");
        assert_eq!(t.focus(Left), Ok(true));
        assert_eq!(t.focused(), Some(2));
    }

    #[test]
    fn focus_entering_container_backwards_along_axis_picks_last_child() {
        let mut t = tree_with(&[1, 2]);
        t.focus_window(1).unwrap();
        t.split(Vertical).unwrap();
        t.add_window(3).unwrap();
        t.focus_window(1).unwrap();
        t.split(Horizontal).unwrap();
        t.add_window(4).unwrap();
        t.focus_window(2).unwrap();
        assert_eq!(t.dump(), "h[v[h[1 4] 3] *2]");
        // Left enters v[..] (orthogonal: first child h[1 4]), then h[1 4]
        // along the axis from the right: last child.
        assert_eq!(t.focus(Left), Ok(true));
        assert_eq!(t.focused(), Some(4));
    }

    #[test]
    fn focus_without_windows_fails() {
        let mut t = Tree::new(Horizontal);
        assert_eq!(t.focus(Left), Err(TreeError::NoFocusedWindow));
    }

    #[test]
    fn layout_flat() {
        let t = tree_with(&[1, 2]);
        assert_eq!(
            t.layout(Rect::new(0, 0, 1000, 500)),
            vec![
                (1, Rect::new(0, 0, 500, 500)),
                (2, Rect::new(500, 0, 500, 500))
            ]
        );
    }

    #[test]
    fn layout_nested() {
        let t = nested(); // h[1 v[2 3]]
        assert_eq!(
            t.layout(Rect::new(0, 0, 1000, 600)),
            vec![
                (1, Rect::new(0, 0, 500, 600)),
                (2, Rect::new(500, 0, 500, 300)),
                (3, Rect::new(500, 300, 500, 300)),
            ]
        );
    }

    #[test]
    fn layout_with_gaps() {
        let t = nested(); // h[1 v[2 3]]
        let gaps = InnerGaps {
            horizontal: 10,
            vertical: 4,
        };
        assert_eq!(
            t.layout_with_gaps(Rect::new(0, 0, 1010, 604), gaps),
            vec![
                (1, Rect::new(0, 0, 500, 604)),
                (2, Rect::new(510, 0, 500, 300)),
                (3, Rect::new(510, 304, 500, 300)),
            ]
        );
    }

    #[test]
    fn toggle_orientation_flips_parent() {
        let mut t = nested(); // h[*1 v[2 3]]
        assert_eq!(t.toggle_orientation(), Ok(Vertical));
        assert_eq!(t.dump(), "v[*1 v[2 3]]");
        t.focus_window(3).unwrap();
        assert_eq!(t.toggle_orientation(), Ok(Horizontal));
        assert_eq!(t.dump(), "v[1 h[2 *3]]");
    }

    #[test]
    fn move_swaps_with_window_sibling() {
        let mut t = tree_with(&[1, 2, 3]);
        assert_eq!(t.move_focused(Left), Ok(true));
        assert_eq!(t.dump(), "h[1 *3 2]");
        assert_eq!(t.move_focused(Left), Ok(true));
        assert_eq!(t.dump(), "h[*3 1 2]");
    }

    #[test]
    fn move_into_container_sibling() {
        let mut t = nested(); // h[*1 v[2 3]]
        assert_eq!(t.move_focused(Right), Ok(true));
        assert_eq!(t.dump(), "v[*1 2 3]", "root absorbs the lone container");
    }

    #[test]
    fn move_out_of_container_to_ancestor() {
        let mut t = nested();
        t.focus_window(3).unwrap(); // h[1 v[2 *3]]
        assert_eq!(t.move_focused(Right), Ok(true));
        assert_eq!(t.dump(), "h[1 2 *3]");

        let mut t = nested();
        t.focus_window(2).unwrap(); // h[1 v[*2 3]]
        assert_eq!(t.move_focused(Left), Ok(true));
        assert_eq!(t.dump(), "h[1 *2 3]");
    }

    #[test]
    fn move_across_root_axis_reorients_root() {
        let mut t = tree_with(&[1, 2, 3]); // h[1 2 *3]
        assert_eq!(t.move_focused(Down), Ok(true));
        assert_eq!(t.dump(), "v[h[1 2] *3]");
        let mut t = tree_with(&[1, 2]); // h[1 *2]
        assert_eq!(t.move_focused(Up), Ok(true));
        assert_eq!(t.dump(), "v[*2 1]");
    }

    #[test]
    fn move_at_edge_of_single_window_tree_is_noop() {
        let mut t = tree_with(&[1]);
        assert_eq!(t.move_focused(Left), Ok(false));
        assert_eq!(t.move_focused(Up), Ok(false));
        assert_eq!(t.dump(), "h[*1]");
    }

    #[test]
    fn move_out_of_pending_split_moves_visibly() {
        // Found by property testing: the window used to leave its lone
        // wrapper and land in the same spot, reporting a move that did not
        // change the layout.
        let mut t = Tree::new(Vertical);
        t.add_window(1).unwrap();
        t.add_window(0).unwrap();
        t.focus_window(1).unwrap();
        t.split(Horizontal).unwrap();
        assert_eq!(t.dump(), "v[h[*1] 0]");
        assert_eq!(t.move_focused(Up), Ok(false), "already at the top");
        assert_eq!(t.dump(), "v[h[*1] 0]");
        assert_eq!(t.move_focused(Down), Ok(true));
        assert_eq!(t.dump(), "v[0 *1]");

        let mut t = tree_with(&[1, 2]);
        t.split(Vertical).unwrap();
        assert_eq!(t.dump(), "h[1 v[*2]]");
        assert_eq!(t.move_focused(Left), Ok(true));
        assert_eq!(t.dump(), "h[*2 1]");

        // A pending split next to the window is swapped with, not entered.
        let mut t = tree_with(&[2, 0]);
        t.split(Horizontal).unwrap();
        t.focus_window(2).unwrap();
        assert_eq!(t.dump(), "h[*2 h[0]]");
        assert_eq!(t.move_focused(Right), Ok(true));
        assert_eq!(t.dump(), "h[h[0] *2]");
    }

    #[test]
    fn move_at_edge_keeps_pending_split() {
        let mut t = tree_with(&[1, 2]);
        t.split(Vertical).unwrap();
        assert_eq!(t.move_focused(Right), Ok(false));
        assert_eq!(t.dump(), "h[1 v[*2]]");
    }

    #[test]
    fn move_at_edge_along_axis_is_noop() {
        let mut t = tree_with(&[1, 2]);
        t.focus_window(1).unwrap();
        assert_eq!(t.move_focused(Left), Ok(false));
        assert_eq!(t.dump(), "h[*1 2]");
    }

    #[test]
    fn node_slots_are_reused() {
        let mut t = tree_with(&[1, 2]);
        let before = t.nodes.len();
        t.remove_window(2).unwrap();
        t.add_window(3).unwrap();
        assert_eq!(t.nodes.len(), before);
    }
}
