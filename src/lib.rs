//! A reference implementation of 2048 on a 4x4 grid.
//!
//! The rules live in [`board`] and [`game`]; [`protocol`] wraps them in a
//! line-oriented command-response interface in the spirit of a chess engine, so
//! a front end can drive a game over stdin/stdout without reimplementing
//! anything.
//!
//! ```
//! use twenty48::{Direction, Game};
//!
//! let mut game = Game::with_seed(42);
//! if game.step(Direction::Left).is_some() {
//!     assert_eq!(game.moves(), 1);
//!     assert!(game.undo());
//! }
//! assert_eq!(game.moves(), 0);
//! ```
//!
//! The crate has no dependencies: randomness is a small seedable generator in
//! [`rng`], so any game replays exactly from its seed.

#![warn(missing_docs)]

pub mod board;
pub mod direction;
pub mod game;
pub mod notation;
pub mod protocol;
pub mod rng;

pub use board::{Board, MAX_EXPONENT, MAX_TILE, SIZE};
pub use direction::Direction;
pub use game::{Game, MAX_SCORE, Move, Status, WIN_EXPONENT};
pub use protocol::Engine;
pub use rng::Rng;
