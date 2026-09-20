# Protocol reference

Protocol revision **1**. This document is normative: it describes exactly what
the engine accepts and exactly what it emits.

## 1. Transport

Requests and responses are lines of UTF-8 on stdin and stdout, separated by
`\n`. A trailing `\r` is stripped, so CRLF input works.

The engine is **strictly synchronous**. It reads one line, writes that line's
complete response, then reads again. It never emits anything unprompted, so
there is no equivalent of UCI's `isready`, `go`, `stop` or `info`.

- A blank or whitespace-only line produces **no response** at all.
- End of input ends the session, exactly like `quit`.
- A line that is not valid UTF-8 is answered `error reason bad-encoding`. The
  session continues and the game in progress is untouched.

The engine starts with **no game loaded**. `move`, `state`, `history`, `undo`
and `redo` answer `error reason no-game` until `newgame`, `setposition` or
`replay` creates one.

## 2. Grammar

### Requests

```
request  = command [ SP argument ]*
command  = "id" | "newgame" | "setposition" | "move" | "state"
         | "history" | "replay" | "undo" | "redo" | "quit"
```

Arguments are `key value` pairs, except `move`, which takes one positional
direction. Consequences:

- **Order is irrelevant.** `newgame seed 42` and a future `newgame seed 42 x y`
  are read the same way.
- **A key with no value is an error** (`bad-argument`, naming the key).
- **An unrecognised key is an error** (`bad-argument`, naming the key). Unknown
  input is never silently ignored.
- **A repeated key takes its last value.**
- Runs of whitespace are separators; leading and trailing whitespace is ignored.

### Responses

```
response = tag [ SP key SP value ]*
tag      = "ok" | "error" | "id" | "idok"
```

**Every value is a single token containing no whitespace.** Lists are joined
with commas; an empty list is the literal `none`. A client can therefore split
any response on whitespace and read it as a tag plus pairs, without knowing
which command produced it.

A multi-line response ends with a terminator line, so future revisions can add
lines before it. Only `id` is multi-line today.

## 3. The state payload

Every command that reports a game returns the same payload, so a client writes
one parser. Written `<state>` below.

```
seed <u64> score <u32> status <status> moves <u32> undoable <u32> redoable <u32> legal <list> board <board>
```

| Key | Meaning |
| --- | --- |
| `seed` | The seed this game runs on. Present on every state report, so any response is enough to reproduce the session. |
| `score` | Current score: the sum of every merged tile's value, plus any starting score. |
| `status` | `playing`, `won` or `over`. See below. |
| `moves` | Moves played. Decreases on `undo`, increases on `redo`. |
| `undoable` | How many moves `undo` can step back through. Always equal to `moves`. |
| `redoable` | How many moves `redo` can step forward into. Follows from nothing else in the payload. |
| `legal` | Directions that would change the board, comma-joined in the order `up,down,left,right`, or `none`. |
| `board` | 16 comma-separated tile values, row-major, `0` for an empty cell. |

`status` values:

- **`playing`** — moves remain, no 2048 tile yet.
- **`won`** — a 2048 tile exists and moves remain. Play continues, as in the
  original game; this is not a terminal state.
- **`over`** — no direction changes the board. `legal` is always `none` here,
  and the two can never disagree.

## 4. Commands

### `id`

Identifies the engine. Takes no arguments.

```
> id
< id name twenty48 version 0.1.1 protocol 1
< idok
```

`protocol` is the revision a client feature-detects on. `version` is the crate
version and carries no compatibility meaning.

### `newgame [seed <u64>]`

Starts a fresh game with two spawned tiles. Without `seed`, one is drawn from
the clock. The seed is **always echoed**, so a clock-seeded session can be
replayed exactly like any other.

```
> newgame seed 42
< ok seed 42 score 0 status playing moves 0 undoable 0 redoable 0 legal up,down,left,right board 0,0,0,0,2,0,0,0,0,0,0,2,0,0,0,0
```

Response: `ok <state>`.

### `setposition board <board> [score <u32>] [seed <u64>]`

Loads a position. `board` is 16 comma-separated tile values, row-major. Nothing
is spawned, `moves` resets to 0, and both history stacks are cleared.

- Each value must be `0` or a power of two from 2 to **131072**; anything else
  is `bad-position`, naming the whole board token.
- `score` defaults to 0 and must not exceed **33554432**; beyond that is
  `bad-argument`.
- `seed` defaults to a clock draw.

```
> setposition board 2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0 score 4 seed 7
< ok seed 7 score 4 status playing moves 0 undoable 0 redoable 0 legal down,left,right board 2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0
```

Response: `ok <state>`.

### `move <direction>`

Plays a move, then spawns a tile. `direction` is `up`, `down`, `left` or
`right` — long spellings only.

```
> move left
< ok moved true gained 4 spawn 1,2,4 seed 7 score 8 status playing moves 1 undoable 1 redoable 0 legal up,down,left,right board 4,0,0,0,0,0,4,0,0,0,0,0,0,0,0,0
```

| Key | Meaning |
| --- | --- |
| `moved` | `true` or `false`. |
| `gained` | Score from this move's merges; `0` for a move that merged nothing. |
| `spawn` | `row,col,value` of the tile placed afterwards, or `none`. |

Followed by `<state>`.

**A direction that changes nothing is not an error.** It answers
`ok moved false gained 0 spawn none` with unchanged state, keeping one response
schema. The `legal` field says in advance which directions will move.

A successful move always frees a cell, so `spawn` is only `none` in a case the
rules cannot produce.

### `state`

Reports the current game. Takes no arguments. Response: `ok <state>`.

### `history`

Reports the single string that reproduces this game, plus the current state.
Takes no arguments. See §6.

```
> history
< ok game g1:q:-:A:B:g seed 42 score 0 status playing moves 1 undoable 1 redoable 0 legal up,down,right board 2,0,0,0,2,0,0,0,2,0,0,0,0,0,0,0
```

Response: `ok game <string> <state>`.

### `replay game <string>`

Rebuilds a game from such a string, replacing any game in progress. Works with
no game loaded. Every move is replayed through the rules, so a string that does
not describe a legal game is rejected rather than producing a different
position.

```
> replay game g1:q:-:A:B:g
< ok seed 42 score 0 status playing moves 1 undoable 1 redoable 0 legal up,down,right board 2,0,0,0,2,0,0,0,2,0,0,0,0,0,0,0
```

Response: `ok <state>`, or `error reason bad-notation detail <what>` — see §5.

### `undo` / `redo`

Steps back through, or forward into, the surviving line of play.

`undo` restores the board, the score, the move count **and the random
generator**, so a move that was undone leaves no trace in the spawn stream.
Undoing and replaying the same move reproduces the identical spawn — undo is
not a reroll. Playing a *different* move after an undo discards the redo stack.

`undoable` and `redoable` in the state payload say in advance whether each
will succeed, exactly as `legal` does for `move`. A client never has to probe
by sending a command it may have to reverse.

Response: `ok <state>`, or `error reason nothing-to-undo` / `nothing-to-redo`.

### `quit`

Ends the session with exit status 0. Emits nothing. Takes no arguments.

## 5. Errors

```
error reason <slug> [ detail <token> ]
```

Slugs are stable kebab-case tokens and are never removed or repurposed. Future
revisions may add new ones, so treat an unrecognised slug as a generic failure.

| Slug | Meaning | `detail` |
| --- | --- | --- |
| `bad-encoding` | The request line was not valid UTF-8. | — |
| `unknown-command` | No such command in this revision. | The command word |
| `bad-argument` | An argument was missing, unrecognised, unparsable or out of range. | The offending token, or the name of the missing key |
| `bad-position` | A board token did not describe a legal position. | The whole board token |
| `bad-notation` | A game string could not be read. | `malformed`, `unknown-version`, `bad-field` or `unplayable` |
| `no-game` | The command needs a game and none is loaded. | — |
| `nothing-to-undo` | The undo stack is empty. | — |
| `nothing-to-redo` | The redo stack is empty. | — |

`bad-notation` details:

| Detail | Meaning |
| --- | --- |
| `malformed` | Not six colon-separated fields. |
| `unknown-version` | The version tag names a format this build does not know. |
| `bad-field` | A field held foreign characters, or a value out of range — including a `count` that disagrees with the move payload. |
| `unplayable` | Well-formed, but the moves do not form a legal game. |

Examples:

```
> bogus
< error reason unknown-command detail bogus
> newgame seed
< error reason bad-argument detail seed
> move sideways
< error reason bad-argument detail sideways
> setposition board 2,4
< error reason bad-position detail 2,4
> replay game nonsense
< error reason bad-notation detail malformed
```

## 6. Game strings

One whitespace-free token reproduces a whole session, spawns included.

```
g1:<seed>:<start>:<score>:<count>:<moves>
```

| Field | Encoding |
| --- | --- |
| `g1` | Format version. A future revision uses a different tag, so a reader can never misread one for the other. |
| `seed` | Integer. |
| `start` | 16 digits, row-major cell **exponents**, one digit per cell — or `-` for a game that began from a seed rather than a loaded position. |
| `score` | Integer: the score the game *started* at, non-zero only for a loaded position. |
| `count` | Integer: how many moves follow. Resolves the final partial group and doubles as a checksum. |
| `moves` | Two bits per move, three moves per character, first move in the high bits. |

### Alphabet

Every field uses **base64url**: `A`–`Z` = 0–25, `a`–`z` = 26–51, `0`–`9` =
52–61, `-` = 62, `_` = 63. It is the densest encoding that stays whitespace-free
and safe in URLs and filenames, avoiding the `+` and `/` of standard base64.

Two consequences worth stating:

- **Zero is `A`, not `0`.** The character `0` is the digit for 52.
- **The board field is range-checked.** The alphabet reaches 63 while a cell
  reaches 17, so `S` is a valid digit naming an impossible tile and is rejected.

`-` is both the absent-start sentinel and the digit 62, but the two cannot be
confused: the sentinel is one character and a position is exactly 16.

### Move codes

| Direction | Code |
| --- | --- |
| `up` | 0 |
| `down` | 1 |
| `left` | 2 |
| `right` | 3 |

Moves are packed most-significant-first into 6-bit characters: move 0 occupies
bits 5–4, move 1 bits 3–2, move 2 bits 1–0. A trailing partial group is padded
with zero bits, and `count` says how many of the decoded moves are real.

### Worked example

```
g1:q:-:A:B:g
```

- `g1` — format version.
- `q` = 42 — seed 42.
- `-` — started from a seed, not a loaded position.
- `A` = 0 — starting score 0.
- `B` = 1 — one move follows.
- `g` = 32 = `0b100000` — bits 5–4 are `10` = 2 = `left`. The remaining slots are
  padding, discarded because `count` is 1.

So: seed 42, `move left`.

### What is recorded

Only the **surviving line of play**. Undo rewinds the generator with the board,
so an undone move leaves no trace in the spawn stream and replaying what remains
is exact.

The **redo stack is session state** and is not carried: `redoable` is 0 after a
`replay`. The undo stack comes back regardless, because `replay` walks every
recorded move through the rules — so `undoable` equals `moves`, as it always
does.

A seed reproduces a game **for this engine**, because spawns come from its own
generator. Another implementation replays a string faithfully only if it draws
spawns identically.

## 7. Limits

| Constant | Value | Meaning |
| --- | --- | --- |
| Grid | 4x4 | |
| Maximum tile | 131072 (`2^17`) | The largest a 4x4 game can produce |
| Maximum score | 33554432 | Bound on any reachable score |
| Winning tile | 2048 (`2^11`) | Sets `status won` |
| Protocol revision | 1 | Reported by `id` |
| Game-string version | `g1` | |

Spawned tiles are 2 with probability 9/10 and 4 with probability 1/10, placed
uniformly at random among empty cells.

## 8. Compatibility

The protocol is **append-only**. These bind every future revision:

1. No command, key, status value or error slug is ever removed or repurposed.
2. New information arrives as a **new key**. Clients must ignore keys they do
   not recognise, and must not depend on key order.
3. Every value stays a single whitespace-free token; lists stay comma-joined;
   empty lists stay `none`.
4. Multi-line responses keep their terminator, and new lines are inserted
   before it.
5. Newly *accepted* input is a compatible change. Short direction spellings, for
   instance, could be added; the long ones could not be taken away.
6. `protocol` from `id` increments only on a change that breaks existing
   clients.
