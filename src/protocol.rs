//! The text command protocol.
//!
//! One command per line, one response per command, strictly synchronous — the
//! engine never speaks unprompted. Responses are key-value pairs whose values
//! are single whitespace-free tokens, so a client can split any response on
//! whitespace and read it without knowing which command produced it.
//!
//! [`Command::History`] and [`Command::Replay`] carry a whole game as one
//! string; see [`crate::notation`] for the format.
//!
//! The protocol is append-only. Commands, keys and status values are never
//! removed or repurposed; new information arrives as a new key, and clients are
//! expected to ignore keys they do not recognise. [`PROTOCOL`] identifies the
//! revision for clients that need to feature-detect.

mod command;

use crate::board::Board;
use crate::direction::Direction;
use crate::game::{Game, Status};
use crate::notation;
use crate::rng;

pub use command::{BAD_ARGUMENT, BAD_NOTATION, BAD_POSITION, Command, ParseError, UNKNOWN_COMMAND};

/// Engine name reported by `id`.
pub const NAME: &str = "game2048";
/// Engine version reported by `id`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Protocol revision reported by `id`.
///
/// Incremented only by a change that breaks existing clients.
pub const PROTOCOL: u32 = 1;

/// The request line was not valid UTF-8.
pub const BAD_ENCODING: &str = "bad-encoding";
/// No game has been started yet.
pub const NO_GAME: &str = "no-game";
/// The undo stack is empty.
pub const NOTHING_TO_UNDO: &str = "nothing-to-undo";
/// The redo stack is empty.
pub const NOTHING_TO_REDO: &str = "nothing-to-redo";

/// What the engine says back to one request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response {
    /// The lines to write, in order. Empty for a blank request.
    pub lines: Vec<String>,
    /// Whether the session should end.
    pub exit: bool,
}

impl Response {
    fn line(line: String) -> Self {
        Self {
            lines: vec![line],
            exit: false,
        }
    }
}

/// A protocol session.
///
/// Deliberately free of I/O: [`Engine::execute`] takes a request line and
/// returns the response, so sessions can be driven from tests as easily as from
/// stdin.
#[derive(Debug, Default)]
pub struct Engine {
    game: Option<Game>,
}

impl Engine {
    /// Creates a session with no game loaded.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the game in progress, if one has been started.
    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    /// Handles one request line given as raw bytes.
    ///
    /// A line that is not valid UTF-8 is answered like any other malformed
    /// request. Ending the session over it would drop the game in progress,
    /// which is a far harsher response than every other bad input gets.
    pub fn execute_bytes(&mut self, line: &[u8]) -> Response {
        match str::from_utf8(line) {
            Ok(text) => self.execute(text),
            Err(_) => Response::line(error_line(BAD_ENCODING, None)),
        }
    }

    /// Handles one request line.
    pub fn execute(&mut self, line: &str) -> Response {
        match command::parse(line) {
            Ok(None) => Response::default(),
            Ok(Some(command)) => self.run(command),
            Err(error) => Response::line(error_line(error.reason, error.detail.as_deref())),
        }
    }

    fn run(&mut self, command: Command) -> Response {
        match command {
            Command::Id => Response {
                lines: vec![
                    format!("id name {NAME} version {VERSION} protocol {PROTOCOL}"),
                    "idok".to_owned(),
                ],
                exit: false,
            },
            Command::NewGame { seed } => self.load(Game::with_seed(resolve(seed))),
            Command::SetPosition { board, score, seed } => {
                self.load(Game::restore(board, score, resolve(seed)))
            }
            Command::Move(direction) => self.with_game(|game| match game.step(direction) {
                Some(played) => format!(
                    "ok moved true gained {} spawn {} {}",
                    played.gained,
                    spawn_token(played.spawned),
                    state_of(game)
                ),
                None => format!("ok moved false gained 0 spawn none {}", state_of(game)),
            }),
            Command::State => self.with_game(|game| format!("ok {}", state_of(game))),
            Command::History => self
                .with_game(|game| format!("ok game {} {}", notation::encode(game), state_of(game))),
            Command::Replay { game } => match notation::decode(&game) {
                Ok(replayed) => self.load(replayed),
                Err(error) => {
                    Response::line(error_line(BAD_NOTATION, Some(notation_detail(error))))
                }
            },
            Command::Undo => self.with_game(|game| {
                if game.undo() {
                    format!("ok {}", state_of(game))
                } else {
                    error_line(NOTHING_TO_UNDO, None)
                }
            }),
            Command::Redo => self.with_game(|game| {
                if game.redo() {
                    format!("ok {}", state_of(game))
                } else {
                    error_line(NOTHING_TO_REDO, None)
                }
            }),
            Command::Quit => Response {
                lines: Vec::new(),
                exit: true,
            },
        }
    }

    /// Installs a game and reports it.
    fn load(&mut self, game: Game) -> Response {
        let line = format!("ok {}", state_of(&game));
        self.game = Some(game);
        Response::line(line)
    }

    fn with_game(&mut self, action: impl FnOnce(&mut Game) -> String) -> Response {
        match self.game.as_mut() {
            Some(game) => Response::line(action(game)),
            None => Response::line(error_line(NO_GAME, None)),
        }
    }
}

/// Uses the requested seed, or draws one from the clock.
fn resolve(seed: Option<u64>) -> u64 {
    seed.unwrap_or_else(rng::entropy_seed)
}

/// Renders the shared state payload returned by every command that reports one.
///
/// The seed lives here rather than only on the commands that start a game, so
/// any response is enough to reproduce the session.
fn state_of(game: &Game) -> String {
    format!(
        "seed {} score {} status {} moves {} legal {} board {}",
        game.seed(),
        game.score(),
        status_token(game.status()),
        game.moves(),
        legal_token(game),
        board_token(game.board()),
    )
}

const fn status_token(status: Status) -> &'static str {
    match status {
        Status::Playing => "playing",
        Status::Won => "won",
        Status::Over => "over",
    }
}

fn legal_token(game: &Game) -> String {
    let names: Vec<&str> = game.available_moves().map(Direction::name).collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(",")
    }
}

fn board_token(board: &Board) -> String {
    board
        .values()
        .iter()
        .flatten()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn spawn_token(spawned: Option<(usize, usize, u32)>) -> String {
    match spawned {
        Some((row, col, value)) => format!("{row},{col},{value}"),
        None => "none".to_owned(),
    }
}

/// Names which part of a game string was wrong.
const fn notation_detail(error: notation::Error) -> &'static str {
    match error {
        notation::Error::Malformed => "malformed",
        notation::Error::UnknownVersion => "unknown-version",
        notation::Error::BadField => "bad-field",
        notation::Error::Unplayable => "unplayable",
    }
}

fn error_line(reason: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("error reason {reason} detail {detail}"),
        None => format!("error reason {reason}"),
    }
}
