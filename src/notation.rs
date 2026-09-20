//! A single string that reproduces a game exactly, spawns included.
//!
//! # Format
//!
//! ```text
//! g1:<seed>:<start>:<score>:<count>:<moves>
//! ```
//!
//! | Field | Encoding |
//! | --- | --- |
//! | `g1` | Format version. A future revision uses a different tag, so a reader can never misread one for the other. |
//! | `seed` | Base64url integer. |
//! | `start` | 16 base64url digits, row-major cell exponents, one per cell — or `-` for a game that began from [`Game::with_seed`]. |
//! | `score` | Base64url integer. The score the game *started* at, which is only non-zero for a loaded position. |
//! | `count` | Base64url integer. How many moves follow, which resolves the final partial group. |
//! | `moves` | Two bits per move, three moves per character, first move in the high bits. |
//!
//! Every field uses one alphabet: base64url (`A`–`Z`, `a`–`z`, `0`–`9`, `-`,
//! `_`). It is the densest encoding that stays whitespace-free and safe in URLs
//! and filenames, avoiding the `+` and `/` of standard base64. The alphabet
//! reaches 63 while a cell reaches [`MAX_EXPONENT`], so the board field is
//! range-checked rather than bounded by its digit set. The `-` marking an
//! absent start is itself a digit, but cannot be confused with a position: the
//! sentinel is one character and a position is exactly 16.
//!
//! Only the surviving line of play is recorded. Undo restores the generator
//! along with the board, so a move that was undone leaves no trace in the
//! spawn stream and replaying the remaining moves is exact.
//!
//! # Scope
//!
//! A seed reproduces a game *for this engine*, because the spawn sequence comes
//! from [`crate::rng`]. Another implementation replays this string faithfully
//! only if it draws spawns the same way.
//!
//! ```
//! use game2048::{Direction, Game, notation};
//!
//! let mut game = Game::with_seed(42);
//! game.step(Direction::Left);
//! game.step(Direction::Up);
//!
//! let encoded = notation::encode(&game);
//! let replayed = notation::decode(&encoded).expect("round trip");
//! assert_eq!(replayed.board(), game.board());
//! assert_eq!(replayed.score(), game.score());
//! ```

use crate::board::{Board, MAX_EXPONENT, SIZE};
use crate::direction::Direction;
use crate::game::{Game, MAX_SCORE};

/// The format tag this module writes.
pub const VERSION: &str = "g1";

/// Marks a game that began from a seed rather than a loaded position.
const NO_START: &str = "-";

/// The base64url alphabet, used by every field.
const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The radix [`DIGITS`] provides.
const RADIX: u64 = DIGITS.len() as u64;

/// Moves packed into one character of the move field.
const MOVES_PER_CHAR: usize = 3;

/// Bits each move occupies.
const BITS_PER_MOVE: u32 = 2;

/// Why a string could not be read as a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The string is not shaped like a notation string at all.
    Malformed,
    /// The version tag names a format this build does not know.
    UnknownVersion,
    /// A field held characters outside its alphabet, or a value out of range.
    BadField,
    /// The moves are legal notation but not a legal game.
    Unplayable,
}

/// Returns the value of one digit, or `None` if it is not one.
fn digit_value(byte: u8) -> Option<u8> {
    DIGITS
        .iter()
        .position(|&digit| digit == byte)
        .map(|value| value as u8)
}

/// Writes `value` in base [`RADIX`], most significant digit first.
fn to_radix(mut value: u64) -> String {
    if value == 0 {
        // Zero is the alphabet's first digit, which is not the character `0`.
        return (DIGITS[0] as char).to_string();
    }

    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % RADIX) as usize]);
        value /= RADIX;
    }
    out.reverse();
    String::from_utf8(out).expect("digits are ASCII")
}

/// Reads a non-empty run of digits, rejecting anything else.
fn from_radix(text: &str) -> Result<u64, Error> {
    if text.is_empty() {
        return Err(Error::BadField);
    }

    text.bytes().try_fold(0u64, |total, byte| {
        let digit = digit_value(byte).ok_or(Error::BadField)?;
        total
            .checked_mul(RADIX)
            .and_then(|shifted| shifted.checked_add(u64::from(digit)))
            .ok_or(Error::BadField)
    })
}

/// Encodes a board as 16 exponent digits, row-major, one per cell.
fn encode_board(board: &Board) -> String {
    let mut out = String::with_capacity(SIZE * SIZE);
    for row in 0..SIZE {
        for col in 0..SIZE {
            let exponent = board.exponent(row, col);
            debug_assert!(exponent <= MAX_EXPONENT);
            out.push(DIGITS[exponent as usize] as char);
        }
    }
    out
}

fn decode_board(text: &str) -> Result<Board, Error> {
    if text.len() != SIZE * SIZE {
        return Err(Error::BadField);
    }

    let mut board = Board::new();
    for (index, byte) in text.bytes().enumerate() {
        // The alphabet reaches 63, so the cap is checked here rather than
        // being implied by the digit set.
        let exponent = digit_value(byte)
            .filter(|&value| value <= MAX_EXPONENT)
            .ok_or(Error::BadField)?;
        board.set_exponent(index / SIZE, index % SIZE, exponent);
    }
    Ok(board)
}

/// Packs moves two bits at a time, three to a character.
fn encode_moves(moves: &[Direction]) -> String {
    moves
        .chunks(MOVES_PER_CHAR)
        .map(|chunk| {
            let packed = chunk
                .iter()
                .enumerate()
                .fold(0u8, |bits, (slot, direction)| {
                    let shift = BITS_PER_MOVE * (MOVES_PER_CHAR - 1 - slot) as u32;
                    bits | (direction.index() << shift)
                });
            DIGITS[packed as usize] as char
        })
        .collect()
}

fn decode_moves(text: &str, count: usize) -> Result<Vec<Direction>, Error> {
    if text.len() != count.div_ceil(MOVES_PER_CHAR) {
        return Err(Error::BadField);
    }

    let mut moves = Vec::with_capacity(count);
    for byte in text.bytes() {
        let packed = digit_value(byte).ok_or(Error::BadField)?;

        for slot in 0..MOVES_PER_CHAR {
            if moves.len() == count {
                break;
            }
            let shift = BITS_PER_MOVE * (MOVES_PER_CHAR - 1 - slot) as u32;
            let code = (packed >> shift) & 0b11;
            moves.push(Direction::from_index(code).ok_or(Error::BadField)?);
        }
    }

    Ok(moves)
}

/// Encodes a game as a single whitespace-free string.
pub fn encode(game: &Game) -> String {
    let moves: Vec<Direction> = game.history().collect();
    let start = game
        .origin()
        .map_or_else(|| NO_START.to_owned(), encode_board);

    format!(
        "{VERSION}:{}:{}:{}:{}:{}",
        to_radix(game.seed()),
        start,
        to_radix(u64::from(game.origin_score())),
        to_radix(moves.len() as u64),
        encode_moves(&moves),
    )
}

/// Rebuilds the game a string describes.
///
/// Every move is replayed through the rules, so a string whose moves do not
/// form a legal game is rejected rather than producing a different position.
pub fn decode(text: &str) -> Result<Game, Error> {
    let fields: Vec<&str> = text.split(':').collect();
    let [version, seed, start, score, count, moves] = fields.as_slice() else {
        return Err(Error::Malformed);
    };

    if *version != VERSION {
        return Err(Error::UnknownVersion);
    }

    let seed = from_radix(seed)?;
    let score = u32::try_from(from_radix(score)?).map_err(|_| Error::BadField)?;
    if score > MAX_SCORE {
        // No game reaches this, and replaying from it could overflow.
        return Err(Error::BadField);
    }
    let count = usize::try_from(from_radix(count)?).map_err(|_| Error::BadField)?;

    let mut game = if *start == NO_START {
        if score != 0 {
            // A seeded game always starts at zero; a non-zero score here would
            // silently disagree with the replay.
            return Err(Error::BadField);
        }
        Game::with_seed(seed)
    } else {
        Game::restore(decode_board(start)?, score, seed)
    };

    for direction in decode_moves(moves, count)? {
        game.step(direction).ok_or(Error::Unplayable)?;
    }

    Ok(game)
}

#[cfg(test)]
mod tests {
    use super::{DIGITS, Error, VERSION, decode, encode, from_radix, to_radix};
    use crate::board::Board;
    use crate::direction::Direction;
    use crate::game::{Game, MAX_SCORE};

    fn play(game: &mut Game, count: usize) {
        for _ in 0..count {
            let Some(direction) = game.available_moves().next() else {
                return;
            };
            game.step(direction).expect("a listed move is legal");
        }
    }

    #[test]
    fn radix_round_trips() {
        for value in [0, 1, 17, 35, 63, 64, 1234, u64::MAX] {
            let text = to_radix(value);
            assert_eq!(from_radix(&text), Ok(value), "{value} should round trip");
        }
    }

    #[test]
    fn radix_rejects_foreign_digits_and_overflow() {
        assert_eq!(from_radix("!"), Err(Error::BadField));
        assert_eq!(from_radix(":"), Err(Error::BadField));
        assert_eq!(from_radix(""), Err(Error::BadField));
        assert_eq!(from_radix(&"_".repeat(20)), Err(Error::BadField));
    }

    #[test]
    fn zero_is_the_alphabets_own_digit_not_the_character_zero() {
        assert_eq!(to_radix(0), "A");
        assert_eq!(from_radix("A"), Ok(0));
        // `0` is a digit too, just not the one meaning zero.
        assert_eq!(from_radix("0"), Ok(52));
    }

    #[test]
    fn the_alphabet_is_base64url_and_excludes_the_delimiter() {
        assert_eq!(DIGITS.len(), 64);
        assert!(!DIGITS.contains(&b'+') && !DIGITS.contains(&b'/'));
        assert!(!DIGITS.iter().any(u8::is_ascii_whitespace));
        assert!(!DIGITS.contains(&b':'), "the delimiter must not be a digit");
    }

    #[test]
    fn a_board_digit_above_the_cap_is_rejected() {
        // `S` is 18, one past MAX_EXPONENT, and a perfectly good base64url
        // digit — so only the range check catches it.
        assert_eq!(
            decode("g1:q:SAAAAAAAAAAAAAAA:A:A:").unwrap_err(),
            Error::BadField
        );
        assert!(
            decode("g1:q:RAAAAAAAAAAAAAAA:A:A:").is_ok(),
            "17 is the cap"
        );
    }

    #[test]
    fn a_fresh_game_encodes_with_no_start_and_no_moves() {
        let game = Game::with_seed(42);
        assert_eq!(encode(&game), format!("{VERSION}:q:-:A:A:"));
    }

    #[test]
    fn the_string_is_a_single_whitespace_free_token() {
        let mut game = Game::with_seed(7);
        play(&mut game, 50);
        let encoded = encode(&game);
        assert!(!encoded.chars().any(char::is_whitespace), "{encoded}");
    }

    #[test]
    fn moves_cost_a_third_of_a_character_each() {
        let mut game = Game::with_seed(7);
        play(&mut game, 30);
        let moves = encode(&game)
            .rsplit(':')
            .next()
            .expect("a moves field")
            .len();
        assert_eq!(moves, 10, "30 moves pack into 10 characters");
    }

    #[test]
    fn a_seeded_game_round_trips() {
        for seed in [0, 1, 42, u64::MAX] {
            let mut original = Game::with_seed(seed);
            play(&mut original, 60);

            let replayed = decode(&encode(&original)).expect("round trip");
            assert_eq!(replayed.board(), original.board());
            assert_eq!(replayed.score(), original.score());
            assert_eq!(replayed.moves(), original.moves());
            assert_eq!(encode(&replayed), encode(&original));
        }
    }

    #[test]
    fn a_loaded_position_round_trips_with_its_score() {
        let board = Board::from_values(&[[2, 4, 0, 0], [0; 4], [0; 4], [0; 4]]).expect("valid");
        let mut original = Game::restore(board, 512, 9);
        play(&mut original, 20);

        let replayed = decode(&encode(&original)).expect("round trip");
        assert_eq!(replayed.board(), original.board());
        assert_eq!(replayed.score(), original.score());
        assert!(replayed.score() >= 512, "the starting score is carried");
    }

    #[test]
    fn every_move_count_round_trips_across_group_boundaries() {
        // 1 and 2 moves leave a partial final group, which is what `count`
        // exists to resolve.
        for count in 0..13 {
            let mut original = Game::with_seed(3);
            play(&mut original, count);

            let replayed = decode(&encode(&original)).expect("round trip");
            assert_eq!(replayed.board(), original.board(), "{count} moves");
            assert_eq!(
                replayed.history().collect::<Vec<_>>(),
                original.history().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn undone_moves_leave_no_trace() {
        let mut game = Game::with_seed(21);
        play(&mut game, 10);
        let expected = encode(&game);

        game.step(Direction::Up);
        game.step(Direction::Down);
        game.undo();
        game.undo();

        assert_eq!(encode(&game), expected, "an undone move is not recorded");
    }

    #[test]
    fn malformed_strings_are_rejected() {
        assert_eq!(decode("").unwrap_err(), Error::Malformed);
        assert_eq!(
            decode("g1:q:-:A:A").unwrap_err(),
            Error::Malformed,
            "too few fields"
        );
        assert_eq!(
            decode("g1:q:-:A:A::").unwrap_err(),
            Error::Malformed,
            "too many"
        );
        assert_eq!(decode("g2:q:-:A:A:").unwrap_err(), Error::UnknownVersion);
        assert_eq!(decode("g1:!!:-:A:A:").unwrap_err(), Error::BadField);
        assert_eq!(
            decode("g1:q:xyz:A:A:").unwrap_err(),
            Error::BadField,
            "short board"
        );
        assert_eq!(
            decode("g1:q:-:F:A:").unwrap_err(),
            Error::BadField,
            "seeded score"
        );
    }

    #[test]
    fn a_score_beyond_any_real_game_is_rejected() {
        let at_cap = to_radix(u64::from(MAX_SCORE));
        let over = to_radix(u64::from(MAX_SCORE) + 1);
        let board = "BAAAAAAAAAAAAAAA";

        assert!(decode(&format!("g1:q:{board}:{at_cap}:A:")).is_ok());
        assert_eq!(
            decode(&format!("g1:q:{board}:{over}:A:")).unwrap_err(),
            Error::BadField
        );
    }

    #[test]
    fn a_move_count_that_disagrees_with_the_moves_is_rejected() {
        assert_eq!(decode("g1:q:-:A:D:").unwrap_err(), Error::BadField);
        assert_eq!(decode("g1:q:-:A:A:AAAA").unwrap_err(), Error::BadField);
    }

    #[test]
    fn a_string_describing_an_illegal_game_is_rejected() {
        // A locked board admits no move at all, so any move is unplayable.
        let locked = "BCBCCBCBBCBCCBCB";
        assert_eq!(
            decode(&format!("g1:B:{locked}:A:B:A")).unwrap_err(),
            Error::Unplayable
        );
    }
}
