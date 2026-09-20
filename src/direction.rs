//! The four directions a move can be played in.

/// A direction the tiles can be shifted towards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Towards row 0.
    Up,
    /// Towards the last row.
    Down,
    /// Towards column 0.
    Left,
    /// Towards the last column.
    Right,
}

impl Direction {
    /// Every direction, in a fixed order.
    pub const ALL: [Direction; 4] = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];

    /// Returns the protocol token for this direction.
    pub const fn name(self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::Down => "down",
            Direction::Left => "left",
            Direction::Right => "right",
        }
    }

    /// Returns this direction's position in [`Direction::ALL`].
    ///
    /// Used as the two-bit code in [`crate::notation`], so the order of `ALL`
    /// is part of the notation format and must never change.
    pub fn index(self) -> u8 {
        Direction::ALL
            .iter()
            .position(|&direction| direction == self)
            .expect("every direction is in ALL") as u8
    }

    /// Returns the direction with this position in [`Direction::ALL`].
    pub fn from_index(index: u8) -> Option<Self> {
        Direction::ALL.get(index as usize).copied()
    }

    /// Parses a protocol token, or returns `None` if it names no direction.
    ///
    /// Only the long spellings are accepted. Recognising more spellings later
    /// is a backwards-compatible change; dropping one would not be.
    pub fn from_name(name: &str) -> Option<Self> {
        Direction::ALL.into_iter().find(|d| d.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::Direction;

    #[test]
    fn names_round_trip() {
        for direction in Direction::ALL {
            assert_eq!(Direction::from_name(direction.name()), Some(direction));
        }
    }

    #[test]
    fn indices_round_trip_and_are_stable() {
        // These codes are baked into the notation format.
        assert_eq!(Direction::Up.index(), 0);
        assert_eq!(Direction::Down.index(), 1);
        assert_eq!(Direction::Left.index(), 2);
        assert_eq!(Direction::Right.index(), 3);

        for direction in Direction::ALL {
            assert_eq!(Direction::from_index(direction.index()), Some(direction));
        }
        assert_eq!(Direction::from_index(4), None);
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(Direction::from_name("u"), None);
        assert_eq!(Direction::from_name(""), None);
        assert_eq!(Direction::from_name("Up"), None);
    }
}
