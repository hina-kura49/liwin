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
        if n == 0 {
            return Vec::new();
        }
        let n_i32 = i32::try_from(n).unwrap_or(i32::MAX);
        let total = match orientation {
            Orientation::Horizontal => self.width,
            Orientation::Vertical => self.height,
        };
        let base = total / n_i32;
        let extra = total % n_i32;
        let mut offset = 0;
        (0..n_i32)
            .map(|i| {
                let size = base + i32::from(i < extra);
                let r = match orientation {
                    Orientation::Horizontal => {
                        Rect::new(self.x + offset, self.y, size, self.height)
                    }
                    Orientation::Vertical => Rect::new(self.x, self.y + offset, self.width, size),
                };
                offset += size;
                r
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
    fn split_zero_is_empty() {
        assert!(
            Rect::new(0, 0, 10, 10)
                .split(Orientation::Vertical, 0)
                .is_empty()
        );
    }

    #[test]
    fn parse_direction() {
        assert_eq!("left".parse::<Direction>(), Ok(Direction::Left));
        assert!("sideways".parse::<Direction>().is_err());
    }
}
