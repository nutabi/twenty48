# twenty48

A 2048 engine for a 4x4 grid, with a line-oriented command protocol in the
spirit of a chess engine.

This is a **reference implementation**. It exists so that front ends — a TUI, a
web UI, a script, a test harness — never reimplement the rules. The rules are a
library; the protocol is a pipe over stdin/stdout. There is no AI or search, and
none is planned.

No dependencies. Spawns come from a small seedable generator, so every game
replays exactly from its seed and every test is deterministic.

## Requirements

Rust 1.85 or newer (the crate uses edition 2024).

## Build and run

```sh
cargo build --release
cargo run            # reads commands on stdin
```

The engine is strictly synchronous: one command per line, one response per
command, and it never speaks unprompted.

```
$ cargo run
id
id name twenty48 version 0.1.0 protocol 1
idok
newgame seed 42
ok seed 42 score 0 status playing moves 0 undoable 0 redoable 0 legal up,down,left,right board 0,0,0,0,2,0,0,0,0,0,0,2,0,0,0,0
move left
ok moved true gained 0 spawn 0,0,2 seed 42 score 0 status playing moves 1 undoable 1 redoable 0 legal up,down,right board 2,0,0,0,2,0,0,0,2,0,0,0,0,0,0,0
quit
```

Every response is a tag followed by `key value` pairs, and every value is a
single whitespace-free token — so a client can split on whitespace and read a
response without knowing which command produced it.

See **[PROTOCOL.md](PROTOCOL.md)** for the exact grammar, every command, every
response field, every error, and the game-string format.

## Reproducing a game

A whole session is one string: seed, optional starting position, score, and the
moves, packed two bits each.

```
history
ok game g1:q:-:A:B:g seed 42 score 0 status playing moves 1 undoable 1 ...
replay game g1:q:-:A:B:g
ok seed 42 score 0 status playing moves 1 undoable 1 ...
```

Undo rewinds the generator along with the board, so a move that was undone
leaves no trace in the spawn stream and only the surviving line of play is
recorded. It also means undo-and-retry cannot reroll a spawn.

## Using it as a library

```rust
use twenty48::{Direction, Game, notation};

let mut game = Game::with_seed(42);
game.step(Direction::Left);

let encoded = notation::encode(&game);
let replayed = notation::decode(&encoded).expect("round trip");
assert_eq!(replayed.board(), game.board());
```

The library surface is deliberately wider than the protocol: `Board::can_shift`,
`Game::available_moves` and `Display for Board` are public for callers that link
against the crate directly. Only the protocol carries a permanence guarantee.

## Tests

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Layout

| Path | Contents |
| --- | --- |
| `src/board.rs` | The grid and the rules: slide, merge, scoring, legality |
| `src/game.rs` | Score, spawns, status, undo/redo stacks, move history |
| `src/direction.rs` | The four directions and their stable codes |
| `src/rng.rs` | Seedable `splitmix64`, with unbiased bounded draws |
| `src/notation.rs` | The single-string game format |
| `src/protocol.rs` | Command dispatch and response rendering |
| `src/protocol/command.rs` | Request parsing |
| `src/main.rs` | The stdin/stdout pipe, and nothing else |

## Design notes

**Cells hold exponents** in a plain `[u8; 16]` — `0` is empty, `n` is the tile
`2^n`, and merging is `n + 1`. A packed `u64` board was considered and rejected:
its real payoff is table-driven search, which is out of scope, and no `u64`
scheme holds the full range anyway — a cell has 18 states, so a board needs
about 66.7 bits.

**Tiles cap at 131072** (`2^17`), the largest a 4x4 game can produce: building
`2^n` means holding the whole staircase below it plus a free cell to spawn into,
which fills the grid exactly at `n = 17`. That last spawn must be a 4, so a game
whose spawns are all 2s tops out one step lower.

**Scores cap at 33,554,432**, derived the same way, which is what keeps a loaded
score from overflowing on the next merge.

**The protocol is append-only.** Nothing shipped is removed or repurposed; new
information arrives as a new key, and clients ignore keys they do not recognise.
`protocol 1` from `id` is what a client feature-detects on.

## License

MIT. See [LICENSE](LICENSE).
