// SPDX-License-Identifier: MIT OR Apache-2.0
//! Basic geometric types.

/// Axis along which a container lays out its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// Children are placed left to right.
    Horizontal,
    /// Children are placed top to bottom.
    Vertical,
}

impl Orientation {
    pub fn flipped(self) -> Self {
        match self {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        }
    }
}

/// A cardinal direction used for focus (and later move) commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// The container orientation along which this direction moves.
    pub fn orientation(self) -> Orientation {
        match self {
            Direction::Left | Direction::Right => Orientation::Horizontal,
            Direction::Up | Direction::Down => Orientation::Vertical,
        }
    }

    /// `true` for directions that move towards higher child indices.
    pub fn is_forward(self) -> bool {
        matches!(self, Direction::Right | Direction::Down)
    }
}

impl std::str::FromStr for Direction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "left" => Ok(Direction::Left),
            "right" => Ok(Direction::Right),
            "up" => Ok(Direction::Up),
            "down" => Ok(Direction::Down),
            other => Err(format!(
                "unknown direction '{other}' (expected left|right|up|down)"
            )),
        }
    }
}

/// An axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    /// Shrinks the rectangle by the given amounts on each side, keeping at
    /// least 1×1 pixel.
    pub fn inset(self, left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect::new(
            self.x + left,
            self.y + top,
            (self.width - left - right).max(1),
            (self.height - top - bottom).max(1),
        )
    }

    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Splits the rectangle into `n` parts along `orientation`.
    ///
    /// Parts are as equal as possible; the leftover pixels go to the first
    /// parts so the union exactly covers `self`.
    pub fn split(self, orientation: Orientation, n: usize) -> Vec<Rect> {
        self.split_with_gap(orientation, n, 0)
    }

    /// Like [`Rect::split`], leaving `gap` pixels between adjacent parts.
    ///
    /// Parts never become narrower than 1 pixel and never leave `self`: when
    /// there is not enough room, the gap shrinks first; if even 1 pixel per
    /// part does not fit, the 1-pixel parts overlap at the far edge.
    pub fn split_with_gap(self, orientation: Orientation, n: usize, gap: i32) -> Vec<Rect> {
        if n == 0 {
            return Vec::new();
        }
        let n = i32::try_from(n).unwrap_or(i32::MAX);
        let length = match orientation {
            Orientation::Horizontal => self.width,
            Orientation::Vertical => self.height,
        }
        .max(1);
        let gap = if n > 1 {
            gap.clamp(0, ((length - n) / (n - 1)).max(0))
        } else {
            0
        };
        let total = (length - gap * (n - 1)).max(n);
        let base = total / n;
        let extra = total % n;
        let mut offset = 0;
        (0..n)
            .map(|i| {
                let size = base + i32::from(i < extra);
                let start = offset.min(length - size);
                offset += size + gap;
                match orientation {
                    Orientation::Horizontal => Rect::new(self.x + start, self.y, size, self.height),
                    Orientation::Vertical => Rect::new(self.x, self.y + start, self.width, size),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_covers_whole_rect() {
        let parts = Rect::new(10, 20, 100, 50).split(Orientation::Horizontal, 3);
        assert_eq!(
            parts,
            vec![
                Rect::new(10, 20, 34, 50),
                Rect::new(44, 20, 33, 50),
                Rect::new(77, 20, 33, 50),
            ]
        );
        let parts = Rect::new(0, 0, 100, 51).split(Orientation::Vertical, 2);
        assert_eq!(
            parts,
            vec![Rect::new(0, 0, 100, 26), Rect::new(0, 26, 100, 25)]
        );
    }

    #[test]
    fn split_with_gap_leaves_space_between_parts() {
        let parts = Rect::new(0, 0, 100, 10).split_with_gap(Orientation::Horizontal, 3, 5);
        assert_eq!(
            parts,
            vec![
                Rect::new(0, 0, 30, 10),
                Rect::new(35, 0, 30, 10),
                Rect::new(70, 0, 30, 10),
            ]
        );
        let tiny = Rect::new(0, 0, 4, 10).split_with_gap(Orientation::Horizontal, 2, 10);
        assert!(tiny.iter().all(|r| r.width >= 1));
    }

    #[test]
    fn inset_shrinks() {
        assert_eq!(
            Rect::new(0, 0, 100, 50).inset(1, 2, 3, 4),
            Rect::new(1, 2, 96, 44)
        );
        assert_eq!(Rect::new(0, 0, 4, 4).inset(10, 10, 10, 10).width, 1);
    }

    #[test]
    fn split_zero_is_empty() {
        assert!(
            Rect::new(0, 0, 10, 10)
                .split(Orientation::Vertical, 0)
                .is_empty()
        );
    }

    #[test]
    fn split_with_gap_shrinks_gap_to_fit() {
        // 2 parts of at least 1 px in 4 px: the 10 px gap shrinks to 2 px
        // instead of pushing the second part out of the rectangle.
        let parts = Rect::new(0, 0, 4, 10).split_with_gap(Orientation::Horizontal, 2, 10);
        assert_eq!(parts, vec![Rect::new(0, 0, 1, 10), Rect::new(3, 0, 1, 10)]);
    }

    #[test]
    fn split_into_more_parts_than_pixels_stays_inside() {
        let parts = Rect::new(5, 0, 2, 10).split(Orientation::Horizontal, 3);
        assert_eq!(
            parts,
            vec![
                Rect::new(5, 0, 1, 10),
                Rect::new(6, 0, 1, 10),
                Rect::new(6, 0, 1, 10),
            ]
        );
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn orientation() -> impl Strategy<Value = Orientation> {
            prop_oneof![Just(Orientation::Horizontal), Just(Orientation::Vertical)]
        }

        /// `(start, length)` of `r` along `o`.
        fn along(r: Rect, o: Orientation) -> (i32, i32) {
            match o {
                Orientation::Horizontal => (r.x, r.width),
                Orientation::Vertical => (r.y, r.height),
            }
        }

        proptest! {
            #[test]
            fn split_with_gap_partitions(
                (x, y, w, h) in (-5000..5000, -5000..5000, 1..5000, 1..5000),
                o in orientation(),
                n in 0usize..64,
                gap in -5i32..200,
            ) {
                let rect = Rect::new(x, y, w, h);
                let parts = rect.split_with_gap(o, n, gap);
                prop_assert_eq!(parts.len(), n);
                let (start, length) = along(rect, o);
                for &p in &parts {
                    // Full extent across the axis, at least 1 px along it,
                    // inside the rectangle.
                    prop_assert_eq!(along(p, o.flipped()), along(rect, o.flipped()));
                    let (s, l) = along(p, o);
                    prop_assert!(l >= 1, "{p:?} is empty");
                    prop_assert!(s >= start && s + l <= start + length, "{p:?} outside {rect:?}");
                }
                if n == 0 || length < n as i32 {
                    return Ok(());
                }
                // Enough room for 1 px per part: the parts are ordered, as
                // equal as possible, separated by one common gap that is the
                // requested one when it fits, and span the whole rectangle.
                let sizes: Vec<i32> = parts.iter().map(|&p| along(p, o).1).collect();
                let min = *sizes.iter().min().unwrap();
                let max = *sizes.iter().max().unwrap();
                prop_assert!(max - min <= 1, "unequal parts: {sizes:?}");
                let gaps: Vec<i32> = parts
                    .windows(2)
                    .map(|pair| {
                        let (s0, l0) = along(pair[0], o);
                        along(pair[1], o).0 - (s0 + l0)
                    })
                    .collect();
                let used = gaps.first().copied().unwrap_or(0);
                prop_assert!(gaps.iter().all(|&g| g == used), "uneven gaps: {gaps:?}");
                prop_assert!((0..=gap.max(0)).contains(&used));
                let needed = i64::from(n as i32) + i64::from(gap.max(0)) * (n as i64 - 1);
                if n > 1 && i64::from(length) >= needed {
                    prop_assert_eq!(used, gap.max(0), "the requested gap fits");
                }
                prop_assert_eq!(along(parts[0], o).0, start);
                let (s, l) = along(*parts.last().unwrap(), o);
                prop_assert_eq!(s + l, start + length);
            }

            #[test]
            fn inset_keeps_a_pixel(
                (x, y, w, h) in (-5000..5000, -5000..5000, 1..5000, 1..5000),
                (l, t, r, b) in (0..3000, 0..3000, 0..3000, 0..3000),
            ) {
                let inner = Rect::new(x, y, w, h).inset(l, t, r, b);
                prop_assert!(inner.width >= 1 && inner.height >= 1);
                if l + r < w && t + b < h {
                    prop_assert_eq!(inner, Rect::new(x + l, y + t, w - l - r, h - t - b));
                }
            }
        }
    }

    #[test]
    fn parse_direction() {
        assert_eq!("left".parse::<Direction>(), Ok(Direction::Left));
        assert!("sideways".parse::<Direction>().is_err());
    }
}
