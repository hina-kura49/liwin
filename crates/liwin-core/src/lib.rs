// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure window-tree and layout logic for liwin.
//!
//! This crate has no knowledge of GNOME, Wayland or D-Bus. It models each
//! workspace as a tree of horizontal/vertical split containers whose leaves
//! are windows, and computes window rectangles from that tree.

pub mod geometry;
pub mod tree;

pub use geometry::{Direction, Orientation, Rect};
pub use tree::{InnerGaps, Tree, TreeError, WindowId};
