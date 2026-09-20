//! Turning a request line into a [`Command`].
//!
//! Arguments are key-value pairs and every value is a single whitespace-free
//! token, so parsing never depends on argument order. Unrecognised keys are
//! rejected rather than ignored: a client sending something this version does
//! not understand should find out immediately.

use crate::board::{Board, SIZE};
use crate::direction::Direction;
use crate::game::MAX_SCORE;

/// The request named a command this version does not have.
pub const UNKNOWN_COMMAND: &str = "unknown-command";
/// An argument was missing, unrecognised or unparsable.
pub const BAD_ARGUMENT: &str = "bad-argument";
/// A board token did not describe a legal position.
pub const BAD_POSITION: &str = "bad-position";
/// A game string could not be read.
pub const BAD_NOTATION: &str = "bad-notation";

/// A parsed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Report name, version and protocol revision.
    Id,
    /// Start a fresh game.
    NewGame {
        /// Seed to play from; drawn from the clock when absent.
        seed: Option<u64>,
    },
    /// Resume from a given position.
    SetPosition {
        /// The position to load.
        board: Board,
        /// The score to resume at.
        score: u32,
        /// Seed to continue from; drawn from the clock when absent.
        seed: Option<u64>,
    },
    /// Play a move.
    Move(Direction),
    /// Report the single string that reproduces this game.
    History,
    /// Rebuild a game from such a string.
    Replay {
        /// The game string to replay.
        game: String,
    },
    /// Report the current state.
    State,
    /// Step back one move.
    Undo,
    /// Replay an undone move.
    Redo,
    /// Leave.
    Quit,
}

/// Why a request line could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// A stable kebab-case slug naming the failure.
    pub reason: &'static str,
    /// The offending token, when one can be pointed at.
    pub detail: Option<String>,
}

impl ParseError {
    fn at(reason: &'static str, detail: &str) -> Self {
        Self {
            reason,
            detail: Some(detail.to_owned()),
        }
    }
}

/// Parses one request line.
///
/// Returns `Ok(None)` for a blank line, which is not a request at all.
pub fn parse(line: &str) -> Result<Option<Command>, ParseError> {
    let mut tokens = line.split_whitespace();
    let Some(name) = tokens.next() else {
        return Ok(None);
    };
    let arguments: Vec<&str> = tokens.collect();

    let command = match name {
        "id" => nullary(Command::Id, &arguments)?,
        "state" => nullary(Command::State, &arguments)?,
        "history" => nullary(Command::History, &arguments)?,
        "replay" => parse_replay(&arguments)?,
        "undo" => nullary(Command::Undo, &arguments)?,
        "redo" => nullary(Command::Redo, &arguments)?,
        "quit" => nullary(Command::Quit, &arguments)?,
        "newgame" => parse_newgame(&arguments)?,
        "setposition" => parse_setposition(&arguments)?,
        "move" => parse_move(&arguments)?,
        other => return Err(ParseError::at(UNKNOWN_COMMAND, other)),
    };

    Ok(Some(command))
}

/// Accepts a command that takes no arguments.
fn nullary(command: Command, arguments: &[&str]) -> Result<Command, ParseError> {
    match arguments.first() {
        Some(unexpected) => Err(ParseError::at(BAD_ARGUMENT, unexpected)),
        None => Ok(command),
    }
}

/// Splits arguments into key-value pairs, rejecting a key with no value.
fn pairs<'a>(arguments: &[&'a str]) -> Result<Vec<(&'a str, &'a str)>, ParseError> {
    match arguments.len() % 2 {
        0 => Ok(arguments.chunks(2).map(|pair| (pair[0], pair[1])).collect()),
        _ => Err(ParseError::at(
            BAD_ARGUMENT,
            arguments.last().expect("an odd count has a last element"),
        )),
    }
}

fn parse_newgame(arguments: &[&str]) -> Result<Command, ParseError> {
    let mut seed = None;
    for (key, value) in pairs(arguments)? {
        match key {
            "seed" => seed = Some(parse_seed(value)?),
            other => return Err(ParseError::at(BAD_ARGUMENT, other)),
        }
    }
    Ok(Command::NewGame { seed })
}

fn parse_setposition(arguments: &[&str]) -> Result<Command, ParseError> {
    let mut board = None;
    let mut score = 0;
    let mut seed = None;

    for (key, value) in pairs(arguments)? {
        match key {
            "board" => board = Some(parse_board(value)?),
            "score" => {
                // Bounded like the board's tiles: a score no game can reach
                // would otherwise overflow on the next merge.
                let parsed: u32 = value
                    .parse()
                    .map_err(|_| ParseError::at(BAD_ARGUMENT, value))?;
                if parsed > MAX_SCORE {
                    return Err(ParseError::at(BAD_ARGUMENT, value));
                }
                score = parsed;
            }
            "seed" => seed = Some(parse_seed(value)?),
            other => return Err(ParseError::at(BAD_ARGUMENT, other)),
        }
    }

    let board = board.ok_or_else(|| ParseError::at(BAD_ARGUMENT, "board"))?;
    Ok(Command::SetPosition { board, score, seed })
}

fn parse_replay(arguments: &[&str]) -> Result<Command, ParseError> {
    let mut game = None;
    for (key, value) in pairs(arguments)? {
        match key {
            "game" => game = Some(value.to_owned()),
            other => return Err(ParseError::at(BAD_ARGUMENT, other)),
        }
    }
    game.map(|game| Command::Replay { game })
        .ok_or_else(|| ParseError::at(BAD_ARGUMENT, "game"))
}

fn parse_move(arguments: &[&str]) -> Result<Command, ParseError> {
    match arguments {
        [name] => Direction::from_name(name)
            .map(Command::Move)
            .ok_or_else(|| ParseError::at(BAD_ARGUMENT, name)),
        [] => Err(ParseError::at(BAD_ARGUMENT, "direction")),
        [_, unexpected, ..] => Err(ParseError::at(BAD_ARGUMENT, unexpected)),
    }
}

fn parse_seed(value: &str) -> Result<u64, ParseError> {
    value
        .parse()
        .map_err(|_| ParseError::at(BAD_ARGUMENT, value))
}

/// Parses the 16 comma-separated tile values of a board token.
fn parse_board(token: &str) -> Result<Board, ParseError> {
    let mut values = [[0; SIZE]; SIZE];
    let mut seen = 0;

    for field in token.split(',') {
        if seen == SIZE * SIZE {
            return Err(ParseError::at(BAD_POSITION, token));
        }
        values[seen / SIZE][seen % SIZE] = field
            .parse()
            .map_err(|_| ParseError::at(BAD_POSITION, token))?;
        seen += 1;
    }

    if seen != SIZE * SIZE {
        return Err(ParseError::at(BAD_POSITION, token));
    }

    Board::from_values(&values).ok_or_else(|| ParseError::at(BAD_POSITION, token))
}

#[cfg(test)]
mod tests {
    use super::{BAD_ARGUMENT, BAD_POSITION, Command, UNKNOWN_COMMAND, parse};
    use crate::direction::Direction;
    use crate::game::MAX_SCORE;

    fn reason(line: &str) -> &'static str {
        parse(line).expect_err("should not parse").reason
    }

    fn detail(line: &str) -> Option<String> {
        parse(line).expect_err("should not parse").detail
    }

    #[test]
    fn blank_lines_are_not_requests() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("   \t "), Ok(None));
    }

    #[test]
    fn nullary_commands_parse() {
        assert_eq!(parse("id"), Ok(Some(Command::Id)));
        assert_eq!(parse("state"), Ok(Some(Command::State)));
        assert_eq!(parse("undo"), Ok(Some(Command::Undo)));
        assert_eq!(parse("redo"), Ok(Some(Command::Redo)));
        assert_eq!(parse("quit"), Ok(Some(Command::Quit)));
    }

    #[test]
    fn surrounding_whitespace_is_irrelevant() {
        assert_eq!(parse("  id  "), Ok(Some(Command::Id)));
        assert_eq!(
            parse("newgame   seed   42"),
            Ok(Some(Command::NewGame { seed: Some(42) }))
        );
    }

    #[test]
    fn newgame_takes_an_optional_seed() {
        assert_eq!(parse("newgame"), Ok(Some(Command::NewGame { seed: None })));
        assert_eq!(
            parse("newgame seed 18446744073709551615"),
            Ok(Some(Command::NewGame {
                seed: Some(u64::MAX)
            }))
        );
    }

    #[test]
    fn move_takes_exactly_one_direction() {
        assert_eq!(parse("move left"), Ok(Some(Command::Move(Direction::Left))));
        assert_eq!(reason("move"), BAD_ARGUMENT);
        assert_eq!(reason("move sideways"), BAD_ARGUMENT);
        assert_eq!(reason("move left right"), BAD_ARGUMENT);
    }

    #[test]
    fn setposition_requires_a_board() {
        let parsed = parse("setposition board 2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,4 score 12 seed 7");
        let Ok(Some(Command::SetPosition { board, score, seed })) = parsed else {
            panic!("should parse, got {parsed:?}");
        };
        assert_eq!(board.value(0, 0), 2);
        assert_eq!(board.value(3, 3), 4);
        assert_eq!(score, 12);
        assert_eq!(seed, Some(7));

        assert_eq!(reason("setposition score 4"), BAD_ARGUMENT);
        assert_eq!(detail("setposition score 4"), Some("board".to_owned()));
    }

    #[test]
    fn a_score_beyond_any_real_game_is_rejected() {
        let board = "2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0";
        assert!(parse(&format!("setposition board {board} score {MAX_SCORE}")).is_ok());
        assert_eq!(
            reason(&format!(
                "setposition board {board} score {}",
                MAX_SCORE + 1
            )),
            BAD_ARGUMENT
        );
        assert_eq!(
            reason(&format!("setposition board {board} score {}", u32::MAX)),
            BAD_ARGUMENT
        );
    }

    #[test]
    fn a_board_token_must_hold_sixteen_legal_tiles() {
        assert_eq!(reason("setposition board 2,4"), BAD_POSITION);
        assert_eq!(
            reason("setposition board 2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,4,8"),
            BAD_POSITION
        );
        assert_eq!(
            reason("setposition board 3,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0"),
            BAD_POSITION
        );
        assert_eq!(
            reason("setposition board x,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0"),
            BAD_POSITION
        );
    }

    #[test]
    fn replay_requires_a_game_string() {
        assert_eq!(
            parse("replay game g1:16:-:0:0:"),
            Ok(Some(Command::Replay {
                game: "g1:16:-:0:0:".to_owned()
            }))
        );
        assert_eq!(reason("replay"), BAD_ARGUMENT);
        assert_eq!(detail("replay"), Some("game".to_owned()));
        assert_eq!(reason("replay seed 4"), BAD_ARGUMENT);
    }

    #[test]
    fn history_takes_no_arguments() {
        assert_eq!(parse("history"), Ok(Some(Command::History)));
        assert_eq!(reason("history all"), BAD_ARGUMENT);
    }

    #[test]
    fn unknown_commands_and_keys_are_rejected_loudly() {
        assert_eq!(reason("go"), UNKNOWN_COMMAND);
        assert_eq!(detail("go"), Some("go".to_owned()));
        assert_eq!(reason("isready"), UNKNOWN_COMMAND);
        assert_eq!(reason("newgame depth 4"), BAD_ARGUMENT);
        assert_eq!(reason("id extra"), BAD_ARGUMENT);
    }

    #[test]
    fn a_key_without_a_value_is_rejected() {
        assert_eq!(reason("newgame seed"), BAD_ARGUMENT);
        assert_eq!(detail("newgame seed"), Some("seed".to_owned()));
    }

    #[test]
    fn an_unparsable_seed_is_rejected() {
        assert_eq!(reason("newgame seed -1"), BAD_ARGUMENT);
        assert_eq!(reason("newgame seed abc"), BAD_ARGUMENT);
    }
}
