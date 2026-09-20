//! A game in progress: the board, the score, the spawn source and the history.

use crate::board::{Board, MAX_EXPONENT, MAX_TILE, SIZE};
use crate::direction::Direction;
use crate::rng::Rng;

/// The exponent of the winning tile, 2048.
pub const WIN_EXPONENT: u8 = 11;

/// Largest score a 4x4 game can reach.
///
/// Building `2^n` entirely from 2-spawns scores `(n - 1) * 2^n`, so a board of
/// 16 maximum tiles bounds the total. No real board holds 16 maximum tiles, so
/// this is a generous ceiling rather than an exact maximum — but it is
/// provable, and it keeps `score + gained` far inside `u32`, which is what
/// stops a loaded score from overflowing on the next merge.
pub const MAX_SCORE: u32 = (MAX_EXPONENT as u32 - 1) * MAX_TILE * (SIZE * SIZE) as u32;

/// Chance, as one in `SPAWN_FOUR_ODDS`, that a spawned tile is a 4 rather than
/// a 2.
const SPAWN_FOUR_ODDS: u32 = 10;

/// How far along a game is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// Moves remain and no 2048 tile exists yet.
    Playing,
    /// A 2048 tile exists and moves remain. Play continues, as in the original
    /// game.
    Won,
    /// No direction changes the board, so the game has ended.
    Over,
}

/// What a successful move did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    /// The direction played.
    pub direction: Direction,
    /// Score gained from merges, which is `0` for a move that merged nothing.
    pub gained: u32,
    /// The `(row, col, value)` of the tile spawned afterwards.
    ///
    /// A successful move always frees a cell, so this is only `None` in the
    /// impossible case of a full board.
    pub spawned: Option<(usize, usize, u32)>,
}

/// A point in the game's history.
///
/// The generator is captured alongside the board, so undoing and replaying
/// reproduces the same spawns, and undoing then playing a *different* move
/// stays deterministic.
#[derive(Debug, Clone)]
struct Snapshot {
    board: Board,
    score: u32,
    moves: u32,
    rng: Rng,
}

/// A game of 2048.
///
/// The history stacks pair each snapshot with the move that connects it to the
/// next state, so the surviving line of play can be read back with
/// [`Game::history`].
#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    score: u32,
    moves: u32,
    rng: Rng,
    /// The position this game was loaded from, if it did not start from a seed.
    origin: Option<Board>,
    /// The score this game started at, which is only non-zero for a loaded
    /// position.
    origin_score: u32,
    /// Pre-move snapshots and the move played from each.
    undone: Vec<(Snapshot, Direction)>,
    /// Post-move snapshots and the move that produced each.
    redone: Vec<(Snapshot, Direction)>,
}

impl Game {
    /// Starts a game from a seed, with two tiles already placed.
    pub fn with_seed(seed: u64) -> Self {
        let mut game = Self {
            board: Board::new(),
            score: 0,
            moves: 0,
            rng: Rng::new(seed),
            origin: None,
            origin_score: 0,
            undone: Vec::new(),
            redone: Vec::new(),
        };
        game.spawn();
        game.spawn();
        game
    }

    /// Resumes a game from a known board, score and seed.
    ///
    /// Nothing is spawned and the history starts empty. A `score` beyond
    /// [`MAX_SCORE`] is accepted but is not something a game can reach;
    /// callers parsing outside input should reject it first.
    pub fn restore(board: Board, score: u32, seed: u64) -> Self {
        Self {
            board,
            score,
            moves: 0,
            rng: Rng::new(seed),
            origin: Some(board),
            origin_score: score,
            undone: Vec::new(),
            redone: Vec::new(),
        }
    }

    /// Returns the position this game was loaded from, or `None` if it started
    /// from a seed alone.
    pub const fn origin(&self) -> Option<&Board> {
        self.origin.as_ref()
    }

    /// Returns the score this game started at.
    pub const fn origin_score(&self) -> u32 {
        self.origin_score
    }

    /// Returns the surviving line of play, oldest move first.
    ///
    /// Undone moves are absent: undo restores the generator along with the
    /// board, so replaying what remains reproduces the game exactly.
    pub fn history(&self) -> impl Iterator<Item = Direction> + '_ {
        self.undone.iter().map(|&(_, direction)| direction)
    }

    /// Returns the board.
    pub const fn board(&self) -> &Board {
        &self.board
    }

    /// Returns the score.
    pub const fn score(&self) -> u32 {
        self.score
    }

    /// Returns how many moves have been played, counting down through undo.
    pub const fn moves(&self) -> u32 {
        self.moves
    }

    /// Returns how many moves [`Game::undo`] can step back through.
    ///
    /// Always equal to [`Game::moves`]: a move pushes onto this stack as it
    /// increments the count, and an undo pops as it restores the decremented
    /// one. Exposed anyway so a caller reads the answer rather than resting on
    /// an invariant it cannot see.
    pub fn undo_count(&self) -> usize {
        self.undone.len()
    }

    /// Returns how many moves [`Game::redo`] can step forward into.
    ///
    /// Unlike [`Game::undo_count`] this follows from nothing else a caller can
    /// observe, so without it the only way to learn whether a redo exists is
    /// to perform one and undo it again.
    pub fn redo_count(&self) -> usize {
        self.redone.len()
    }

    /// Returns the seed this game runs on.
    pub const fn seed(&self) -> u64 {
        self.rng.seed()
    }

    /// Returns how far along the game is.
    ///
    /// A finished game reports [`Status::Over`] even if it reached 2048; the
    /// winning tile is still on the board for a caller that cares.
    pub fn status(&self) -> Status {
        if !self.board.has_moves() {
            Status::Over
        } else if self.board.max_exponent() >= WIN_EXPONENT {
            Status::Won
        } else {
            Status::Playing
        }
    }

    /// Returns the directions that would change the board.
    pub fn available_moves(&self) -> impl Iterator<Item = Direction> + '_ {
        Direction::ALL
            .into_iter()
            .filter(|&direction| self.board.can_shift(direction))
    }

    /// Plays a move, then spawns a tile.
    ///
    /// Returns `None` without touching the game if the direction changes
    /// nothing. A successful move clears the redo stack.
    pub fn step(&mut self, direction: Direction) -> Option<Move> {
        let previous = self.snapshot();
        let gained = self.board.shift(direction)?;

        // Saturating so a score loaded through the library API cannot panic.
        // Input arriving over the protocol is bounded by `MAX_SCORE`, which
        // puts this far out of reach.
        self.score = self.score.saturating_add(gained);
        self.moves += 1;
        self.undone.push((previous, direction));
        self.redone.clear();

        let spawned = self.spawn();
        Some(Move {
            direction,
            gained,
            spawned,
        })
    }

    /// Steps back one move, returning whether there was one to step back to.
    pub fn undo(&mut self) -> bool {
        let Some((previous, direction)) = self.undone.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.restore_snapshot(previous);
        self.redone.push((current, direction));
        true
    }

    /// Replays an undone move, returning whether there was one to replay.
    pub fn redo(&mut self) -> bool {
        let Some((next, direction)) = self.redone.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.restore_snapshot(next);
        self.undone.push((current, direction));
        true
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            board: self.board,
            score: self.score,
            moves: self.moves,
            rng: self.rng.clone(),
        }
    }

    fn restore_snapshot(&mut self, snapshot: Snapshot) {
        self.board = snapshot.board;
        self.score = snapshot.score;
        self.moves = snapshot.moves;
        self.rng = snapshot.rng;
    }

    /// Places a 2 or a 4 on a random empty cell.
    fn spawn(&mut self) -> Option<(usize, usize, u32)> {
        let empty = u32::try_from(self.board.empty_count()).ok()?;
        if empty == 0 {
            return None;
        }

        let pick = self.rng.below(empty) as usize;
        let (row, col) = self.board.empty_cells().nth(pick)?;
        let exponent = if self.rng.below(SPAWN_FOUR_ODDS) == 0 {
            2
        } else {
            1
        };

        self.board.set_exponent(row, col, exponent);
        Some((row, col, 1 << exponent))
    }
}

#[cfg(test)]
mod tests {
    use super::{Game, MAX_SCORE, Status};
    use crate::board::{Board, MAX_TILE};
    use crate::direction::Direction;

    fn board(values: [[u32; 4]; 4]) -> Board {
        Board::from_values(&values).expect("test board should be valid")
    }

    #[test]
    fn a_new_game_starts_with_two_tiles() {
        let game = Game::with_seed(42);
        assert_eq!(game.board().empty_count(), 14);
        assert_eq!(game.score(), 0);
        assert_eq!(game.moves(), 0);
        assert_eq!(game.status(), Status::Playing);
    }

    #[test]
    fn spawned_tiles_are_twos_or_fours() {
        for seed in 0..200 {
            let game = Game::with_seed(seed);
            for row in 0..4 {
                for col in 0..4 {
                    let value = game.board().value(row, col);
                    assert!(
                        matches!(value, 0 | 2 | 4),
                        "seed {seed} spawned {value} at ({row}, {col})"
                    );
                }
            }
        }
    }

    #[test]
    fn the_same_seed_replays_the_same_game() {
        let mut left = Game::with_seed(7);
        let mut right = Game::with_seed(7);

        for _ in 0..40 {
            for direction in Direction::ALL {
                assert_eq!(left.step(direction), right.step(direction));
            }
        }

        assert_eq!(left.board(), right.board());
        assert_eq!(left.score(), right.score());
    }

    #[test]
    fn a_move_that_changes_nothing_is_refused() {
        // Already packed left with no equal neighbours.
        let mut game = Game::restore(
            board([
                [2, 4, 8, 16],
                [4, 8, 16, 32],
                [8, 16, 32, 64],
                [16, 32, 64, 128],
            ]),
            0,
            1,
        );
        assert_eq!(game.step(Direction::Left), None);
        assert_eq!(game.moves(), 0);
        assert!(!game.undo(), "a refused move leaves no history");
    }

    #[test]
    fn undo_restores_the_board_the_score_and_the_move_count() {
        let mut game = Game::with_seed(3);
        let before = *game.board();

        Direction::ALL
            .into_iter()
            .find(|&direction| game.step(direction).is_some())
            .expect("a fresh game has a legal move");

        assert_ne!(*game.board(), before);
        assert!(game.undo());
        assert_eq!(*game.board(), before);
        assert_eq!(game.score(), 0);
        assert_eq!(game.moves(), 0);
    }

    /// Returns any direction that changes the board.
    fn any_legal(game: &Game) -> Direction {
        game.available_moves()
            .next()
            .expect("the game should still be playable")
    }

    #[test]
    fn undo_then_redo_reproduces_the_same_spawn() {
        let mut game = Game::with_seed(11);
        let direction = any_legal(&game);
        let first = game.step(direction).expect("legal move");
        let after = *game.board();

        assert!(game.undo());
        assert!(game.redo());

        assert_eq!(
            *game.board(),
            after,
            "redo must reproduce the spawn exactly"
        );
        assert_eq!(game.score(), first.gained);
        assert_eq!(game.moves(), 1);
    }

    #[test]
    fn undoing_then_playing_again_clears_the_redo_stack() {
        let mut game = Game::with_seed(5);
        let direction = any_legal(&game);

        game.step(direction).expect("legal move");
        assert!(game.undo());

        let replayed = any_legal(&game);
        game.step(replayed).expect("legal move");

        assert!(!game.redo(), "a new move discards the redone future");
    }

    #[test]
    fn undo_rewinds_the_generator_so_a_retried_move_cannot_be_rerolled() {
        // The property the single-string notation rests on: a move that was
        // undone leaves no trace in the spawn stream.
        let mut undone = Game::with_seed(31);
        let first = any_legal(&undone);
        undone.step(first).expect("legal move");
        undone.undo();

        let second = any_legal(&undone);
        undone.step(second).expect("legal move");

        let mut direct = Game::with_seed(31);
        direct.step(second).expect("legal move");

        assert_eq!(
            undone.board(),
            direct.board(),
            "playing a move after an undo must match playing it outright"
        );
        assert_eq!(undone.score(), direct.score());
    }

    #[test]
    fn replaying_the_same_move_after_undo_reproduces_the_same_spawn() {
        // No savescumming: undo then retry is not a reroll.
        let mut game = Game::with_seed(17);
        let direction = any_legal(&game);

        game.step(direction).expect("legal move");
        let first = *game.board();

        game.undo();
        game.step(direction).expect("legal move");

        assert_eq!(*game.board(), first);
    }

    #[test]
    fn history_records_the_surviving_line_of_play() {
        let mut game = Game::with_seed(23);
        let mut played = Vec::new();

        for _ in 0..5 {
            let direction = any_legal(&game);
            game.step(direction).expect("legal move");
            played.push(direction);
        }
        assert_eq!(game.history().collect::<Vec<_>>(), played);

        game.undo();
        played.pop();
        assert_eq!(game.history().collect::<Vec<_>>(), played, "undo drops it");

        game.redo();
        assert_eq!(game.history().count(), played.len() + 1, "redo restores it");
    }

    #[test]
    fn the_origin_is_remembered_only_for_a_loaded_position() {
        let seeded = Game::with_seed(1);
        assert_eq!(seeded.origin(), None);
        assert_eq!(seeded.origin_score(), 0);

        let start = board([[2, 4, 0, 0], [0; 4], [0; 4], [0; 4]]);
        let mut loaded = Game::restore(start, 64, 1);
        loaded.step(Direction::Right).expect("legal move");

        assert_eq!(loaded.origin(), Some(&start), "the origin never moves");
        assert_eq!(loaded.origin_score(), 64);
    }

    #[test]
    fn a_loaded_score_cannot_overflow_on_the_next_merge() {
        // Two 65536 tiles merge for 131072 points on top of a huge score.
        let start = board([[65536, 65536, 0, 0], [0; 4], [0; 4], [0; 4]]);
        let mut game = Game::restore(start, u32::MAX - 5, 1);

        game.step(Direction::Left).expect("legal move");
        assert_eq!(game.score(), u32::MAX, "saturates rather than wrapping");
    }

    #[test]
    fn the_score_ceiling_leaves_room_for_any_single_merge() {
        assert_eq!(MAX_SCORE, 33_554_432);
        assert!(
            MAX_SCORE.checked_add(MAX_TILE).is_some(),
            "a merge on top of the ceiling must still fit"
        );
    }

    #[test]
    fn the_stack_depths_follow_undo_and_redo() {
        let mut game = Game::with_seed(13);
        assert_eq!((game.undo_count(), game.redo_count()), (0, 0));

        for _ in 0..3 {
            let direction = any_legal(&game);
            game.step(direction).expect("legal move");
        }
        assert_eq!((game.undo_count(), game.redo_count()), (3, 0));

        assert!(game.undo());
        assert_eq!((game.undo_count(), game.redo_count()), (2, 1));

        assert!(game.redo());
        assert_eq!((game.undo_count(), game.redo_count()), (3, 0));
    }

    #[test]
    fn the_undo_depth_never_disagrees_with_the_move_count() {
        // The protocol documents `undoable` as always equal to `moves`, not
        // merely usually, so the invariant is asserted rather than implied.
        let mut game = Game::with_seed(29);

        for _ in 0..40 {
            let Some(direction) = game.available_moves().next() else {
                break;
            };
            game.step(direction).expect("a listed move is legal");
            assert_eq!(game.undo_count() as u32, game.moves(), "after a move");
        }
        while game.undo() {
            assert_eq!(game.undo_count() as u32, game.moves(), "after an undo");
        }
        while game.redo() {
            assert_eq!(game.undo_count() as u32, game.moves(), "after a redo");
        }
    }

    #[test]
    fn a_new_move_empties_the_redo_depth() {
        let mut game = Game::with_seed(5);
        let direction = any_legal(&game);

        game.step(direction).expect("legal move");
        assert!(game.undo());
        assert_eq!(game.redo_count(), 1, "the undone move is redoable");

        let replayed = any_legal(&game);
        game.step(replayed).expect("legal move");
        assert_eq!(game.redo_count(), 0, "a new move discards it");
    }

    #[test]
    fn a_restored_position_starts_with_both_stacks_empty() {
        let game = Game::restore(board([[2, 4, 0, 0], [0; 4], [0; 4], [0; 4]]), 64, 1);
        assert_eq!((game.undo_count(), game.redo_count()), (0, 0));
    }

    #[test]
    fn nothing_to_undo_or_redo_on_a_fresh_game() {
        let mut game = Game::with_seed(1);
        assert!(!game.undo());
        assert!(!game.redo());
    }

    #[test]
    fn reaching_2048_wins_without_ending_the_game() {
        let game = Game::restore(board([[2048, 0, 0, 0], [0; 4], [0; 4], [0; 4]]), 0, 1);
        assert_eq!(game.status(), Status::Won);
        assert!(game.available_moves().count() > 0);
    }

    #[test]
    fn a_locked_board_is_over() {
        let game = Game::restore(
            board([[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]]),
            0,
            1,
        );
        assert_eq!(game.status(), Status::Over);
        assert_eq!(game.available_moves().count(), 0);
    }

    #[test]
    fn available_moves_matches_what_step_accepts() {
        let game = Game::restore(board([[2, 2, 0, 0], [0; 4], [0; 4], [0; 4]]), 0, 1);
        let legal: Vec<_> = game.available_moves().collect();
        assert!(legal.contains(&Direction::Left));

        for direction in Direction::ALL {
            let mut probe = game.clone();
            assert_eq!(probe.step(direction).is_some(), legal.contains(&direction));
        }
    }
}
