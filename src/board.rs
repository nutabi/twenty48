//! The 4x4 grid and the rules that govern it.
//!
//! Cells hold *exponents*, not tile values: `0` is an empty cell and `n` is the
//! tile `2^n`. Merging two equal tiles is then `n + 1`, and the grid stays a
//! plain `[u8; 16]` that reads like the rules it implements.

use core::fmt;

use crate::direction::Direction;

/// Width and height of the grid.
pub const SIZE: usize = 4;

const CELLS: usize = SIZE * SIZE;

/// Largest exponent a cell may hold: the largest tile a 4x4 game can produce.
///
/// Creating `2^n` requires holding the whole staircase below it — `2^(n-1)`,
/// `2^(n-2)`, down to a spawn-sized tile — plus one free cell to spawn into,
/// which costs a cell per step. The peak position is therefore `65536, 32768,
/// … 4` (15 tiles) with one cell left for a spawned 4, filling the grid exactly
/// and cascading up to `2^17`. That final spawn must be a 4, so a game whose
/// spawns are all 2s tops out one step lower, at 65536.
///
/// Positions loaded from outside are held to this bound, and [`Board::shift`]
/// preserves it: a pair of maximum tiles does not merge. Such a position is
/// itself unreachable in play, so refusing keeps the invariant exact rather
/// than quietly admitting a tile no game can produce.
pub const MAX_EXPONENT: u8 = 17;

/// Largest tile value a cell may hold, `2^MAX_EXPONENT` — 131072.
pub const MAX_TILE: u32 = 1 << MAX_EXPONENT;

/// A 4x4 grid of tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Board {
    /// Row-major exponents; `0` marks an empty cell.
    cells: [u8; CELLS],
}

/// Returns the tile value for an exponent, where `0` means an empty cell.
const fn value_of(exponent: u8) -> u32 {
    if exponent == 0 { 0 } else { 1 << exponent }
}

/// Returns the exponent for a tile value, or `None` if no tile can hold it.
///
/// Rejects anything that is not empty or a power of two from 2 to [`MAX_TILE`].
fn exponent_of(value: u32) -> Option<u8> {
    match value {
        0 => Some(0),
        value if value >= 2 && value.is_power_of_two() && value <= MAX_TILE => {
            Some(value.trailing_zeros() as u8)
        }
        _ => None,
    }
}

/// Maps a position within a line to a cell index.
///
/// `line` selects the row or column and `step` walks it *against* the shift, so
/// step 0 is the cell tiles pile up against. That lets all four directions share
/// one collapse routine without transposing the grid.
const fn position(direction: Direction, line: usize, step: usize) -> usize {
    match direction {
        Direction::Left => line * SIZE + step,
        Direction::Right => line * SIZE + (SIZE - 1 - step),
        Direction::Up => step * SIZE + line,
        Direction::Down => (SIZE - 1 - step) * SIZE + line,
    }
}

/// Slides a single line towards step 0, merging equal neighbours.
///
/// Returns the new line and the score gained. A tile that was just formed by a
/// merge cannot merge again in the same move, so `4 4 4 4` collapses to `8 8`
/// rather than `16`.
fn collapse(line: [u8; SIZE]) -> ([u8; SIZE], u32) {
    let mut out = [0; SIZE];
    let mut gained = 0;
    let mut write = 0;
    let mut merged = false;

    for tile in line.into_iter().filter(|&tile| tile != 0) {
        // `tile < MAX_EXPONENT` holds the board to the largest attainable tile.
        // Two maximum tiles cannot coexist in a real game, so this only ever
        // bites on a hand-loaded position.
        if !merged && write > 0 && out[write - 1] == tile && tile < MAX_EXPONENT {
            out[write - 1] = tile + 1;
            gained += value_of(tile + 1);
            merged = true;
        } else {
            out[write] = tile;
            write += 1;
            merged = false;
        }
    }

    (out, gained)
}

impl Board {
    /// Creates an empty board.
    pub const fn new() -> Self {
        Self { cells: [0; CELLS] }
    }

    /// Builds a board from tile values, or returns `None` if any value is
    /// neither `0` nor a power of two from 2 to [`MAX_TILE`].
    pub fn from_values(values: &[[u32; SIZE]; SIZE]) -> Option<Self> {
        let mut board = Self::new();
        for (row, line) in values.iter().enumerate() {
            for (col, &value) in line.iter().enumerate() {
                board.cells[row * SIZE + col] = exponent_of(value)?;
            }
        }
        Some(board)
    }

    /// Returns the grid as tile values, with `0` for empty cells.
    pub fn values(&self) -> [[u32; SIZE]; SIZE] {
        let mut values = [[0; SIZE]; SIZE];
        for (row, line) in values.iter_mut().enumerate() {
            for (col, value) in line.iter_mut().enumerate() {
                *value = value_of(self.cells[row * SIZE + col]);
            }
        }
        values
    }

    /// Returns the exponent at a cell, where `0` means empty.
    pub const fn exponent(&self, row: usize, col: usize) -> u8 {
        self.cells[row * SIZE + col]
    }

    /// Returns the tile value at a cell, where `0` means empty.
    pub const fn value(&self, row: usize, col: usize) -> u32 {
        value_of(self.exponent(row, col))
    }

    /// Places an exponent at a cell.
    ///
    /// # Panics
    ///
    /// Panics if `exponent` exceeds [`MAX_EXPONENT`].
    pub fn set_exponent(&mut self, row: usize, col: usize, exponent: u8) {
        assert!(
            exponent <= MAX_EXPONENT,
            "exponent {exponent} is not representable"
        );
        self.cells[row * SIZE + col] = exponent;
    }

    /// Returns the number of empty cells.
    pub fn empty_count(&self) -> usize {
        self.cells.iter().filter(|&&tile| tile == 0).count()
    }

    /// Returns the `(row, col)` of every empty cell, in row-major order.
    pub fn empty_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter(|&(_, &tile)| tile == 0)
            .map(|(index, _)| (index / SIZE, index % SIZE))
    }

    /// Returns the largest exponent on the board, or `0` if it is empty.
    pub fn max_exponent(&self) -> u8 {
        self.cells.iter().copied().max().unwrap_or(0)
    }

    /// Slides every tile towards `direction`, merging equal neighbours.
    ///
    /// Returns `None` if nothing moved, otherwise the score gained — which is
    /// `Some(0)` for a legal move that merged nothing.
    pub fn shift(&mut self, direction: Direction) -> Option<u32> {
        let mut gained = 0;
        let mut moved = false;

        for line in 0..SIZE {
            let mut before = [0; SIZE];
            for (step, tile) in before.iter_mut().enumerate() {
                *tile = self.cells[position(direction, line, step)];
            }

            let (after, points) = collapse(before);
            if after != before {
                moved = true;
            }
            gained += points;

            for (step, &tile) in after.iter().enumerate() {
                self.cells[position(direction, line, step)] = tile;
            }
        }

        moved.then_some(gained)
    }

    /// Returns whether shifting towards `direction` would change the board.
    pub fn can_shift(&self, direction: Direction) -> bool {
        let mut probe = *self;
        probe.shift(direction).is_some()
    }

    /// Returns whether any direction is still playable.
    ///
    /// Defined in terms of [`Board::can_shift`] so it cannot drift from the
    /// rules. A neighbour scan would be faster, but it has to re-derive when a
    /// move exists and then disagrees at the edges — an empty board has empty
    /// cells yet no move, and a pair of maximum tiles is equal yet cannot
    /// merge. Those are exactly the cases where a contradictory
    /// `status playing` / `legal none` response would escape.
    pub fn has_moves(&self) -> bool {
        Direction::ALL
            .into_iter()
            .any(|direction| self.can_shift(direction))
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for row in 0..SIZE {
            for col in 0..SIZE {
                match self.value(row, col) {
                    0 => write!(f, "{:>7}", ".")?,
                    value => write!(f, "{value:>7}")?,
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Board, MAX_TILE, collapse};
    use crate::direction::Direction;

    /// Builds a board, panicking on values no tile can hold.
    fn board(values: [[u32; 4]; 4]) -> Board {
        Board::from_values(&values).expect("test board should be valid")
    }

    #[test]
    fn collapse_slides_without_merging() {
        let (line, gained) = collapse([0, 1, 0, 2]);
        assert_eq!(line, [1, 2, 0, 0]);
        assert_eq!(gained, 0);
    }

    #[test]
    fn collapse_merges_a_pair_and_scores_the_result() {
        // Two 4s (exponent 2) become an 8.
        let (line, gained) = collapse([2, 2, 0, 0]);
        assert_eq!(line, [3, 0, 0, 0]);
        assert_eq!(gained, 8);
    }

    #[test]
    fn a_merged_tile_cannot_merge_again_in_the_same_move() {
        let (line, gained) = collapse([1, 1, 1, 1]);
        assert_eq!(line, [2, 2, 0, 0]);
        assert_eq!(gained, 8, "two separate merges of 2 + 2");
    }

    #[test]
    fn merging_starts_from_the_shift_edge() {
        // 2 2 4 shifting left merges the leading pair, not the trailing one.
        let (line, _) = collapse([1, 1, 2, 0]);
        assert_eq!(line, [2, 2, 0, 0]);
    }

    #[test]
    fn shift_reports_no_move_when_nothing_changes() {
        let mut grid = board([[2, 4, 8, 16], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        assert_eq!(grid.shift(Direction::Left), None);
    }

    #[test]
    fn shift_reports_a_move_that_merges_nothing() {
        let mut grid = board([[0, 0, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        assert_eq!(grid.shift(Direction::Left), Some(0));
        assert_eq!(grid.values()[0], [2, 0, 0, 0]);
    }

    #[test]
    fn every_direction_piles_tiles_against_its_own_edge() {
        let start = board([[0, 0, 0, 0], [0, 2, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);

        let mut left = start;
        left.shift(Direction::Left);
        assert_eq!(left.value(1, 0), 2);

        let mut right = start;
        right.shift(Direction::Right);
        assert_eq!(right.value(1, 3), 2);

        let mut up = start;
        up.shift(Direction::Up);
        assert_eq!(up.value(0, 1), 2);

        let mut down = start;
        down.shift(Direction::Down);
        assert_eq!(down.value(3, 1), 2);
    }

    #[test]
    fn shift_scores_every_merge_in_the_move() {
        let mut grid = board([[2, 2, 4, 4], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        assert_eq!(grid.shift(Direction::Left), Some(4 + 8));
        assert_eq!(grid.values()[0], [4, 8, 0, 0]);
    }

    #[test]
    fn an_interlocked_full_board_has_no_moves() {
        let grid = board([[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]]);
        assert!(!grid.has_moves());
        assert!(Direction::ALL.iter().all(|&d| !grid.can_shift(d)));
    }

    #[test]
    fn has_moves_never_disagrees_with_can_shift() {
        let cases = [
            ("empty", [[0; 4], [0; 4], [0; 4], [0; 4]]),
            ("one tile", [[2, 0, 0, 0], [0; 4], [0; 4], [0; 4]]),
            (
                "locked",
                [[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]],
            ),
            (
                "two maximum tiles, adjacent",
                [
                    [MAX_TILE, MAX_TILE, 2, 4],
                    [4, 2, 4, 2],
                    [2, 4, 2, 4],
                    [4, 2, 4, 2],
                ],
            ),
        ];

        for (name, values) in cases {
            let grid = board(values);
            let any = Direction::ALL.iter().any(|&d| grid.can_shift(d));
            assert_eq!(grid.has_moves(), any, "{name}");
        }
    }

    #[test]
    fn an_empty_board_has_no_moves() {
        assert!(!Board::new().has_moves(), "nothing to slide");
    }

    #[test]
    fn a_pair_of_maximum_tiles_is_not_a_move() {
        // They are equal neighbours, but `collapse` refuses to merge them.
        let grid = board([
            [MAX_TILE, MAX_TILE, 2, 4],
            [4, 2, 4, 2],
            [2, 4, 2, 4],
            [4, 2, 4, 2],
        ]);
        assert!(!grid.has_moves());
    }

    #[test]
    fn a_full_board_with_equal_neighbours_still_has_moves() {
        let grid = board([[2, 2, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]]);
        assert!(grid.has_moves());
    }

    #[test]
    fn values_round_trip() {
        let values = [
            [0, 2, 4, 8],
            [16, 32, 64, 128],
            [256, 512, 1024, 2048],
            [4096, 0, 0, 2],
        ];
        assert_eq!(board(values).values(), values);
    }

    #[test]
    fn from_values_rejects_impossible_tiles() {
        assert!(Board::from_values(&[[3, 0, 0, 0], [0; 4], [0; 4], [0; 4]]).is_none());
        assert!(Board::from_values(&[[1, 0, 0, 0], [0; 4], [0; 4], [0; 4]]).is_none());
    }

    #[test]
    fn the_largest_attainable_tile_is_accepted_and_anything_above_is_not() {
        assert_eq!(MAX_TILE, 131_072);
        assert!(Board::from_values(&[[MAX_TILE, 0, 0, 0], [0; 4], [0; 4], [0; 4]]).is_some());
        assert!(Board::from_values(&[[MAX_TILE * 2, 0, 0, 0], [0; 4], [0; 4], [0; 4]]).is_none());
    }

    #[test]
    fn a_pair_of_maximum_tiles_does_not_merge_past_the_cap() {
        let mut grid = board([[MAX_TILE, MAX_TILE, 0, 0], [0; 4], [0; 4], [0; 4]]);
        assert_eq!(
            grid.shift(Direction::Left),
            None,
            "the move changes nothing"
        );
        assert_eq!(grid.value(0, 0), MAX_TILE);
        assert_eq!(grid.value(0, 1), MAX_TILE);
    }

    #[test]
    fn merging_up_to_the_cap_still_works() {
        let half = MAX_TILE / 2;
        let mut grid = board([[half, half, 0, 0], [0; 4], [0; 4], [0; 4]]);
        assert_eq!(grid.shift(Direction::Left), Some(MAX_TILE));
        assert_eq!(grid.value(0, 0), MAX_TILE);
    }

    #[test]
    fn empty_cells_are_listed_in_row_major_order() {
        let grid = board([[2, 0, 0, 0], [0, 4, 0, 0], [0; 4], [0; 4]]);
        assert_eq!(grid.empty_count(), 14);
        assert_eq!(grid.empty_cells().next(), Some((0, 1)));
        assert_eq!(grid.max_exponent(), 2);
    }
}
