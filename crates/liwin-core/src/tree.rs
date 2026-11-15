//! The per-workspace container tree.
//!
//! A workspace is a tree whose inner nodes are split containers (horizontal or
//! vertical) and whose leaves are windows. The root is always a container and
//! is never removed, even when empty.
//!
//! Invariants maintained after every public operation:
//! - every non-root container has at least two children (normalization);
//! - the root never has a single child that is itself a container;
//! - `focused` is `Some` iff the tree contains at least one window.

use std::collections::HashMap;
use std::fmt;

use crate::geometry::{Direction, Orientation, Rect};

/// Identifier of a window as given by the compositor (`Meta.Window.get_id()`).
pub type WindowId = u64;

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
    /// focused, focus moves to its previous sibling (or the next one).
    pub fn remove_window(&mut self, window: WindowId) -> Result<(), TreeError> {
        let node = self
            .windows
            .remove(&window)
            .ok_or(TreeError::NoSuchWindow(window))?;
        let (parent, index) = self.position(node).expect("window node has a parent");
        self.children_mut(parent).remove(index);
        self.release(node);

        if self.focused == Some(node) {
            let siblings = self.children(parent);
            self.focused = if index > 0 {
                Some(self.last_leaf(siblings[index - 1]))
            } else if let Some(&next) = siblings.first() {
                Some(self.first_leaf(next))
            } else {
                None
            };
        }

        self.normalize_upwards(parent);

        if self.focused.is_none() && !self.windows.is_empty() {
            self.focused = Some(self.first_leaf(self.root));
        }
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
            if self.orientation(parent) == direction.orientation() {
                let len = self.children(parent).len();
                let target = if direction.is_forward() {
                    (index + 1 < len).then_some(index + 1)
                } else {
                    index.checked_sub(1)
                };
                if let Some(target) = target {
                    let sibling = self.children(parent)[target];
                    self.focused = Some(self.descend(sibling, direction));
                    return Ok(true);
                }
            }
            current = parent;
        }
        Ok(false)
    }

    /// Computes the rectangle of every window when the root occupies `area`.
    /// Children of a container share its space equally.
    pub fn layout(&self, area: Rect) -> Vec<(WindowId, Rect)> {
        let mut out = Vec::with_capacity(self.windows.len());
        self.layout_node(self.root, area, &mut out);
        out
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

    fn layout_node(&self, id: NodeId, area: Rect, out: &mut Vec<(WindowId, Rect)>) {
        match &self.node(id).kind {
            Kind::Window(w) => out.push((*w, area)),
            Kind::Container {
                orientation,
                children,
            } => {
                for (&child, rect) in children
                    .iter()
                    .zip(area.split(*orientation, children.len()))
                {
                    self.layout_node(child, rect, out);
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
    fn node_slots_are_reused() {
        let mut t = tree_with(&[1, 2]);
        let before = t.nodes.len();
        t.remove_window(2).unwrap();
        t.add_window(3).unwrap();
        assert_eq!(t.nodes.len(), before);
    }
}
