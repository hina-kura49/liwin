// SPDX-License-Identifier: MIT OR Apache-2.0
//! Property tests: random operation sequences must keep the tree's
//! invariants and produce sane layouts.

use std::collections::BTreeSet;

use proptest::prelude::*;

use super::*;

/// Window ids are drawn from a small range so that adds, removes and focus
/// commands regularly hit existing (and missing) windows.
const MAX_ID: WindowId = 10;

#[derive(Debug, Clone)]
enum Op {
    Add(WindowId),
    Remove(WindowId),
    FocusWindow(WindowId),
    Focus(Direction),
    Move(Direction),
    Split(Orientation),
    Toggle,
}

fn direction() -> impl Strategy<Value = Direction> {
    prop_oneof![
        Just(Direction::Left),
        Just(Direction::Right),
        Just(Direction::Up),
        Just(Direction::Down),
    ]
}

fn orientation() -> impl Strategy<Value = Orientation> {
    prop_oneof![Just(Orientation::Horizontal), Just(Orientation::Vertical)]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..MAX_ID).prop_map(Op::Add),
        2 => (0..MAX_ID).prop_map(Op::Remove),
        1 => (0..MAX_ID).prop_map(Op::FocusWindow),
        3 => direction().prop_map(Op::Focus),
        3 => direction().prop_map(Op::Move),
        2 => orientation().prop_map(Op::Split),
        1 => Just(Op::Toggle),
    ]
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op(), 0..80)
}

fn right(r: Rect) -> i64 {
    i64::from(r.x) + i64::from(r.width)
}

fn bottom(r: Rect) -> i64 {
    i64::from(r.y) + i64::from(r.height)
}

fn area_of(r: Rect) -> i64 {
    i64::from(r.width) * i64::from(r.height)
}

fn inside(r: Rect, area: Rect) -> bool {
    r.x >= area.x && r.y >= area.y && right(r) <= right(area) && bottom(r) <= bottom(area)
}

fn overlap(a: Rect, b: Rect) -> bool {
    i64::from(a.x) < right(b)
        && i64::from(b.x) < right(a)
        && i64::from(a.y) < bottom(b)
        && i64::from(b.y) < bottom(a)
}

/// `true` if `a` and `b` share a segment of their boundary.
fn touching(a: Rect, b: Rect) -> bool {
    let x_overlap = i64::from(a.x) < right(b) && i64::from(b.x) < right(a);
    let y_overlap = i64::from(a.y) < bottom(b) && i64::from(b.y) < bottom(a);
    let x_adjacent = right(a) == i64::from(b.x) || right(b) == i64::from(a.x);
    let y_adjacent = bottom(a) == i64::from(b.y) || bottom(b) == i64::from(a.y);
    (x_adjacent && y_overlap) || (y_adjacent && x_overlap)
}

/// Smallest `(width, height)` for which the equal-split layout of `id`
/// gives every window at least 1×1 pixel and keeps the full gaps.
fn min_size(t: &Tree, id: NodeId, gaps: InnerGaps) -> (i64, i64) {
    match &t.node(id).kind {
        Kind::Window(_) => (1, 1),
        Kind::Container {
            orientation,
            children,
        } => {
            let sizes: Vec<_> = children.iter().map(|&c| min_size(t, c, gaps)).collect();
            let n = sizes.len() as i64;
            let max_w = sizes.iter().map(|s| s.0).max().unwrap_or(0);
            let max_h = sizes.iter().map(|s| s.1).max().unwrap_or(0);
            match orientation {
                Orientation::Horizontal => (
                    n * max_w + (n - 1).max(0) * i64::from(gaps.horizontal),
                    max_h,
                ),
                Orientation::Vertical => {
                    (max_w, n * max_h + (n - 1).max(0) * i64::from(gaps.vertical))
                }
            }
        }
    }
}

/// Checks `layout_with_gaps(area, gaps)` against the tree's windows.
fn check_layout(t: &Tree, area: Rect, gaps: InnerGaps) -> Result<(), TestCaseError> {
    let layout = t.layout_with_gaps(area, gaps);
    let ids: Vec<_> = layout.iter().map(|&(w, _)| w).collect();
    prop_assert_eq!(&ids, &t.windows(), "layout lists windows in tree order");

    for &(w, r) in &layout {
        prop_assert!(r.width >= 1 && r.height >= 1, "{w}: {r:?} is empty");
        prop_assert!(inside(r, area), "{w}: {r:?} outside {area:?}");
    }

    let (min_w, min_h) = min_size(t, t.root, gaps);
    if i64::from(area.width) >= min_w && i64::from(area.height) >= min_h {
        for (i, &(a, ra)) in layout.iter().enumerate() {
            for &(b, rb) in &layout[i + 1..] {
                prop_assert!(!overlap(ra, rb), "{a}: {ra:?} overlaps {b}: {rb:?}");
            }
        }
        if gaps == InnerGaps::default() && !layout.is_empty() {
            let covered: i64 = layout.iter().map(|&(_, r)| area_of(r)).sum();
            prop_assert_eq!(covered, area_of(area), "layout covers the area exactly");
        }
    }
    Ok(())
}

/// A screen large enough for any tree of at most `MAX_ID` windows to be
/// laid out without overlap.
const LARGE: Rect = Rect::new(0, 0, 1 << 20, 1 << 20);

/// Rectangle of `window` in a zero-gap layout of [`LARGE`].
fn rect_of(t: &Tree, window: WindowId) -> Rect {
    t.layout(LARGE)
        .into_iter()
        .find(|&(w, _)| w == window)
        .map(|(_, r)| r)
        .expect("window is laid out")
}

/// Applies `op` to `t`, checking its result against the `model` set of
/// windows, and updates the model.
fn apply(t: &mut Tree, model: &mut BTreeSet<WindowId>, op: &Op) -> Result<(), TestCaseError> {
    let before = t.dump();
    let focused = t.focused();
    match *op {
        Op::Add(w) => {
            let res = t.add_window(w);
            if model.insert(w) {
                prop_assert_eq!(res, Ok(()));
                prop_assert_eq!(t.focused(), Some(w), "a new window is focused");
            } else {
                prop_assert_eq!(res, Err(TreeError::DuplicateWindow(w)));
                prop_assert_eq!(t.dump(), before, "failed add leaves the tree alone");
            }
        }
        Op::Remove(w) => {
            let layout_before = t.layout(LARGE);
            let res = t.remove_window(w);
            if model.remove(&w) {
                prop_assert_eq!(res, Ok(()));
                if focused != Some(w) {
                    prop_assert_eq!(t.focused(), focused, "unfocused removal keeps focus");
                } else if let Some(new) = t.focused() {
                    // Focus goes to a window that was touching the removed one.
                    let rect = |id| layout_before.iter().find(|&&(x, _)| x == id).unwrap().1;
                    prop_assert!(
                        touching(rect(w), rect(new)),
                        "removing {} from {} focused {}",
                        w,
                        before,
                        new
                    );
                }
            } else {
                prop_assert_eq!(res, Err(TreeError::NoSuchWindow(w)));
                prop_assert_eq!(t.dump(), before, "failed remove leaves the tree alone");
            }
        }
        Op::FocusWindow(w) => {
            let res = t.focus_window(w);
            if model.contains(&w) {
                prop_assert_eq!(res, Ok(()));
                prop_assert_eq!(t.focused(), Some(w));
            } else {
                prop_assert_eq!(res, Err(TreeError::NoSuchWindow(w)));
                prop_assert_eq!(t.focused(), focused);
            }
        }
        Op::Focus(d) => {
            let res = t.focus(d);
            match focused {
                None => prop_assert_eq!(res, Err(TreeError::NoFocusedWindow)),
                Some(old) => {
                    // In a zero-gap layout, focus moves to a window whose
                    // facing edge touches the old window's edge, and fails
                    // only at the edge of the screen.
                    let old_rect = rect_of(t, old);
                    let old_edge = match d {
                        Direction::Left => i64::from(old_rect.x),
                        Direction::Right => right(old_rect),
                        Direction::Up => i64::from(old_rect.y),
                        Direction::Down => bottom(old_rect),
                    };
                    let screen_edge = match d {
                        Direction::Left | Direction::Up => 0,
                        Direction::Right => right(LARGE),
                        Direction::Down => bottom(LARGE),
                    };
                    match res {
                        Ok(false) => {
                            prop_assert_eq!(t.focused(), Some(old));
                            prop_assert_eq!(old_edge, screen_edge, "focus {:?} stopped early", d);
                        }
                        Ok(true) => {
                            let new = t.focused().expect("focus stays set");
                            prop_assert_ne!(new, old);
                            let r = rect_of(t, new);
                            let new_edge = match d {
                                Direction::Left => right(r),
                                Direction::Right => i64::from(r.x),
                                Direction::Up => bottom(r),
                                Direction::Down => i64::from(r.y),
                            };
                            prop_assert_eq!(
                                new_edge,
                                old_edge,
                                "focus {:?} went from {:?} to {:?}",
                                d,
                                old_rect,
                                r
                            );
                        }
                        Err(e) => return Err(TestCaseError::fail(format!("focus failed: {e}"))),
                    }
                }
            }
            prop_assert_eq!(t.dump().replace('*', ""), before.replace('*', ""));
        }
        Op::Move(d) => {
            let old_rect = focused.map(|w| rect_of(t, w));
            let res = t.move_focused(d);
            match (focused, old_rect) {
                (Some(old), Some(old_rect)) => {
                    prop_assert!(res.is_ok());
                    prop_assert_eq!(t.focused(), Some(old), "the moved window keeps focus");
                    if res == Ok(false) {
                        prop_assert_eq!(t.dump(), before, "a no-op move changes nothing");
                    } else {
                        let r = rect_of(t, old);
                        prop_assert_ne!(r, old_rect, "move {:?} in {} had no effect", d, before);
                    }
                }
                _ => prop_assert_eq!(res, Err(TreeError::NoFocusedWindow)),
            }
        }
        Op::Split(o) => {
            let res = t.split(o);
            match focused {
                None => prop_assert_eq!(res, Err(TreeError::NoFocusedWindow)),
                Some(_) => {
                    prop_assert_eq!(res, Ok(()));
                    prop_assert_eq!(t.focused_container_orientation(), Ok(o));
                }
            }
            prop_assert_eq!(t.focused(), focused);
        }
        Op::Toggle => {
            let old = t.focused_container_orientation();
            let res = t.toggle_orientation();
            prop_assert_eq!(&res, &old.map(Orientation::flipped));
            if let Ok(o) = res {
                prop_assert_eq!(t.focused_container_orientation(), Ok(o));
            }
            prop_assert_eq!(t.focused(), focused);
        }
    }
    Ok(())
}

fn check_state(t: &Tree, model: &BTreeSet<WindowId>) -> Result<(), TestCaseError> {
    t.check_invariants().map_err(TestCaseError::fail)?;
    prop_assert_eq!(t.len(), model.len());
    prop_assert_eq!(t.is_empty(), model.is_empty());
    let windows: BTreeSet<_> = t.windows().into_iter().collect();
    prop_assert_eq!(&windows, model);
    for w in 0..MAX_ID {
        prop_assert_eq!(t.contains(w), model.contains(&w));
    }
    match t.focused() {
        Some(f) => prop_assert!(model.contains(&f)),
        None => prop_assert!(model.is_empty()),
    }
    let (w, h) = min_size(t, t.root, InnerGaps::default());
    prop_assert!(w <= i64::from(LARGE.width) && h <= i64::from(LARGE.height));
    check_layout(t, LARGE, InnerGaps::default())
}

fn gaps() -> impl Strategy<Value = InnerGaps> {
    (0..40, 0..40).prop_map(|(horizontal, vertical)| InnerGaps {
        horizontal,
        vertical,
    })
}

fn area() -> impl Strategy<Value = Rect> {
    (-2000..2000, -2000..2000, 1..5000, 1..5000).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

proptest! {
    #[test]
    fn operations_keep_invariants(
        root in orientation(),
        ops in ops(),
        area in area(),
        gaps in gaps(),
    ) {
        let mut t = Tree::new(root);
        let mut model = BTreeSet::new();
        check_state(&t, &model)?;
        for op in &ops {
            apply(&mut t, &mut model, op)?;
            check_state(&t, &model)?;
            check_layout(&t, area, InnerGaps::default())?;
            check_layout(&t, area, gaps)?;
        }
    }
}
