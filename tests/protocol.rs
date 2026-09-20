//! Protocol-level tests.
//!
//! These guard the wire format rather than the rules, because the format is the
//! part that can never change: every command, key and status value shipped here
//! is permanent.

use std::collections::HashSet;

use game2048::Engine;

/// A board already packed to the left, so shifting left changes nothing.
const PACKED_LEFT: &str = "2,4,8,16,4,8,16,32,8,16,32,64,16,32,64,128";
/// A checkerboard with no equal neighbours: no move is legal.
const LOCKED: &str = "2,4,2,4,4,2,4,2,2,4,2,4,4,2,4,2";

/// Runs a script, returning every line the engine emitted.
fn run(script: &[&str]) -> Vec<String> {
    let mut engine = Engine::new();
    let mut output = Vec::new();
    for line in script {
        let response = engine.execute(line);
        output.extend(response.lines);
        if response.exit {
            break;
        }
    }
    output
}

/// Runs a script and returns only the last line, which is the usual assertion.
fn last(script: &[&str]) -> String {
    run(script).pop().expect("the script should produce output")
}

/// Reads a response line as `tag` plus key-value pairs.
///
/// Panics unless the line satisfies the format contract, which is the whole
/// reason clients can parse responses they do not fully understand.
fn parse_response(line: &str) -> (String, Vec<(String, String)>) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let (tag, rest) = tokens.split_first().expect("a response is never empty");

    assert_eq!(
        rest.len() % 2,
        0,
        "response must be a tag followed by key-value pairs: {line}"
    );

    let pairs: Vec<(String, String)> = rest
        .chunks(2)
        .map(|pair| (pair[0].to_owned(), pair[1].to_owned()))
        .collect();

    let mut keys = HashSet::new();
    for (key, _) in &pairs {
        assert!(keys.insert(key.clone()), "duplicate key {key} in: {line}");
    }

    ((*tag).to_owned(), pairs)
}

/// Returns the value of a key in a response line.
fn field(line: &str, key: &str) -> String {
    let (_, pairs) = parse_response(line);
    pairs
        .into_iter()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("no key {key} in: {line}"))
        .1
}

#[test]
fn id_reports_the_protocol_revision_and_terminates() {
    let lines = run(&["id"]);
    assert_eq!(lines.len(), 2);
    assert_eq!(field(&lines[0], "name"), "game2048");
    assert_eq!(field(&lines[0], "protocol"), "1");
    assert_eq!(
        field(&lines[0], "version"),
        env!("CARGO_PKG_VERSION"),
        "id must report the crate version"
    );
    assert_eq!(lines[1], "idok", "the block must end in a terminator");
}

#[test]
fn nothing_works_before_a_game_exists() {
    for command in ["move left", "state", "undo", "redo"] {
        assert_eq!(
            last(&[command]),
            "error reason no-game",
            "{command} should require a game"
        );
    }
}

#[test]
fn id_and_quit_work_without_a_game() {
    assert_eq!(run(&["id"]).len(), 2);
    assert!(run(&["quit"]).is_empty(), "quit answers nothing");
}

#[test]
fn quit_ends_the_session() {
    let output = run(&["newgame seed 1", "quit", "id"]);
    assert_eq!(output.len(), 1, "nothing after quit should run");
}

#[test]
fn blank_lines_produce_no_response() {
    assert!(run(&["", "   "]).is_empty());
}

#[test]
fn newgame_echoes_its_seed_and_opens_with_two_tiles() {
    let line = last(&["newgame seed 42"]);
    assert_eq!(field(&line, "seed"), "42");
    assert_eq!(field(&line, "score"), "0");
    assert_eq!(field(&line, "moves"), "0");
    assert_eq!(field(&line, "status"), "playing");

    let tiles = field(&line, "board")
        .split(',')
        .filter(|value| *value != "0")
        .count();
    assert_eq!(tiles, 2);
}

#[test]
fn a_clock_seeded_game_still_reports_a_usable_seed() {
    let seed = field(&last(&["newgame"]), "seed");
    let replay = last(&[&format!("newgame seed {seed}")]);
    assert_eq!(
        field(&replay, "board"),
        field(&last(&[&format!("newgame seed {seed}")]), "board"),
        "the reported seed must reproduce the game"
    );
}

#[test]
fn the_same_seed_produces_byte_identical_sessions() {
    let script = [
        "id",
        "newgame seed 42",
        "move left",
        "move up",
        "move right",
        "state",
        "undo",
        "redo",
        "quit",
    ];
    assert_eq!(run(&script), run(&script));
}

#[test]
fn a_move_reports_what_it_did() {
    let line = last(&[
        "setposition board 2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0 seed 7",
        "move left",
    ]);
    let (tag, _) = parse_response(&line);

    assert_eq!(tag, "ok");
    assert_eq!(field(&line, "moved"), "true");
    assert_eq!(field(&line, "gained"), "4", "2 + 2 scores 4");
    assert_eq!(field(&line, "score"), "4");
    assert_eq!(field(&line, "moves"), "1");

    let spawn = field(&line, "spawn");
    let spawn: Vec<&str> = spawn.split(',').collect();
    assert_eq!(spawn.len(), 3, "spawn is row,col,value");
    assert!(matches!(spawn[2], "2" | "4"));
}

#[test]
fn an_illegal_move_is_an_outcome_not_an_error() {
    let start = format!("setposition board {PACKED_LEFT} score 10 seed 7");
    let before = last(&[&start]);
    let after = last(&[&start, "move left"]);

    assert_eq!(field(&after, "moved"), "false");
    assert_eq!(field(&after, "gained"), "0");
    assert_eq!(field(&after, "spawn"), "none");
    assert_eq!(field(&after, "moves"), "0", "a refused move is not counted");
    assert_eq!(
        field(&after, "board"),
        field(&before, "board"),
        "a refused move leaves the board alone"
    );
    assert_eq!(parse_response(&after).0, "ok", "still an ok response");
}

#[test]
fn setposition_restores_score_and_leaves_the_board_untouched() {
    let line = last(&[&format!("setposition board {PACKED_LEFT} score 99 seed 3")]);
    assert_eq!(field(&line, "score"), "99");
    assert_eq!(field(&line, "board"), PACKED_LEFT, "nothing is spawned");
    assert_eq!(field(&line, "moves"), "0");
}

#[test]
fn a_locked_board_is_over_with_no_legal_moves() {
    let line = last(&[&format!("setposition board {LOCKED} seed 1")]);
    assert_eq!(field(&line, "status"), "over");
    assert_eq!(field(&line, "legal"), "none");
}

#[test]
fn reaching_2048_reports_won_while_play_continues() {
    let line = last(&["setposition board 2048,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0 seed 1"]);
    assert_eq!(field(&line, "status"), "won");
    assert_ne!(
        field(&line, "legal"),
        "none",
        "a won game is still playable"
    );
}

#[test]
fn legal_lists_only_directions_that_change_the_board() {
    let line = last(&["setposition board 2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0 seed 1"]);
    let legal = field(&line, "legal");
    let legal: HashSet<&str> = legal.split(',').collect();
    assert_eq!(legal, HashSet::from(["down", "right"]));
}

#[test]
fn undo_and_redo_walk_the_history() {
    let script = [
        "setposition board 2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0 seed 7",
        "move left",
        "undo",
        "redo",
    ];
    let lines = run(&script);
    let (opened, moved, undone, redone) = (&lines[0], &lines[1], &lines[2], &lines[3]);

    assert_eq!(field(undone, "board"), field(opened, "board"));
    assert_eq!(field(undone, "score"), "0");
    assert_eq!(field(undone, "moves"), "0");

    assert_eq!(
        field(redone, "board"),
        field(moved, "board"),
        "redo must reproduce the spawn exactly"
    );
    assert_eq!(field(redone, "score"), field(moved, "score"));
}

#[test]
fn empty_history_reports_the_right_slug() {
    let start = "newgame seed 1";
    assert_eq!(last(&[start, "undo"]), "error reason nothing-to-undo");
    assert_eq!(last(&[start, "redo"]), "error reason nothing-to-redo");
}

#[test]
fn setposition_clears_the_history() {
    let script = [
        "newgame seed 1",
        "move left",
        &format!("setposition board {PACKED_LEFT} seed 1"),
        "undo",
    ];
    let script: Vec<&str> = script.iter().map(AsRef::as_ref).collect();
    assert_eq!(last(&script), "error reason nothing-to-undo");
}

#[test]
fn malformed_requests_name_a_stable_slug_and_the_offending_token() {
    assert_eq!(last(&["go"]), "error reason unknown-command detail go");
    assert_eq!(
        last(&["isready"]),
        "error reason unknown-command detail isready"
    );
    assert_eq!(
        last(&["newgame depth 4"]),
        "error reason bad-argument detail depth"
    );
    assert_eq!(
        last(&["newgame seed"]),
        "error reason bad-argument detail seed"
    );
    assert_eq!(
        last(&["newgame seed 1", "move sideways"]),
        "error reason bad-argument detail sideways"
    );
    assert_eq!(
        last(&["setposition board 2,4"]),
        "error reason bad-position detail 2,4"
    );
}

#[test]
fn setposition_holds_tiles_to_the_largest_attainable_value() {
    let at_cap = "131072,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0";
    assert_eq!(
        field(&last(&[&format!("setposition board {at_cap}")]), "board"),
        at_cap
    );

    for rejected in [
        "262144,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0",
        "1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0",
        "6,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0",
    ] {
        assert_eq!(
            last(&[&format!("setposition board {rejected}")]),
            format!("error reason bad-position detail {rejected}")
        );
    }
}

#[test]
fn a_game_string_reproduces_the_session_exactly() {
    let played = run(&[
        "newgame seed 42",
        "move left",
        "move up",
        "move right",
        "move down",
        "history",
    ]);
    let source = played.last().expect("history answers");
    let game = field(source, "game");

    // A brand new session, given only the string, must land on the same state.
    let replayed = last(&[&format!("replay game {game}")]);
    for key in ["score", "status", "moves", "legal", "board", "seed"] {
        assert_eq!(
            field(&replayed, key),
            field(source, key),
            "{key} must survive the round trip"
        );
    }

    let again = last(&[&format!("replay game {game}"), "history"]);
    assert_eq!(field(&again, "game"), game, "the string is stable");
}

#[test]
fn a_game_string_is_one_whitespace_free_token() {
    let line = last(&["newgame seed 42", "move left", "history"]);
    let game = field(&line, "game");
    assert!(!game.chars().any(char::is_whitespace), "{game}");
    assert!(game.starts_with("g1:"), "versioned: {game}");
}

#[test]
fn a_loaded_position_round_trips_through_its_string() {
    let start = format!("setposition board {PACKED_LEFT} score 250 seed 9");
    let source = last(&[&start, "move right", "history"]);
    let game = field(&source, "game");

    let replayed = last(&[&format!("replay game {game}")]);
    assert_eq!(field(&replayed, "board"), field(&source, "board"));
    assert_eq!(field(&replayed, "score"), field(&source, "score"));
    assert!(
        field(&replayed, "score").parse::<u32>().expect("a number") >= 250,
        "the starting score is carried"
    );
}

#[test]
fn undone_moves_leave_no_trace_in_the_string() {
    let base = ["newgame seed 42", "move left", "history"];
    let expected = field(last(&base).as_str(), "game");

    let script = [
        "newgame seed 42",
        "move left",
        "move up",
        "move right",
        "undo",
        "undo",
        "history",
    ];
    assert_eq!(field(&last(&script), "game"), expected);
}

#[test]
fn replay_works_without_a_game_and_history_does_not() {
    assert_eq!(last(&["history"]), "error reason no-game");

    let replayed = last(&["replay game g1:q:-:A:A:"]);
    assert_eq!(parse_response(&replayed).0, "ok");
    assert_eq!(field(&replayed, "seed"), "42", "base64url: q is 42");
}

#[test]
fn a_bad_game_string_names_what_was_wrong() {
    let cases = [
        ("replay game nonsense", "malformed"),
        ("replay game g9:q:-:A:A:", "unknown-version"),
        ("replay game g1:!!:-:A:A:", "bad-field"),
        ("replay game g1:q:-:A:D:", "bad-field"),
        ("replay game g1:B:BCBCCBCBBCBCCBCB:A:B:A", "unplayable"),
    ];
    for (request, detail) in cases {
        assert_eq!(
            last(&[request]),
            format!("error reason bad-notation detail {detail}"),
            "{request}"
        );
    }
}

#[test]
fn a_line_that_is_not_utf8_is_answered_not_fatal() {
    let mut engine = Engine::new();
    engine.execute("newgame seed 42");
    let before = engine.execute("state").lines.remove(0);

    // A lone 0xFF is never valid UTF-8.
    let response = engine.execute_bytes(&[b'm', b'o', b'v', b'e', b' ', 0xFF]);
    assert_eq!(response.lines, vec!["error reason bad-encoding"]);
    assert!(!response.exit, "the session must survive");

    let after = engine.execute("state").lines.remove(0);
    assert_eq!(after, before, "the game is untouched");
}

#[test]
fn valid_utf8_bytes_behave_exactly_like_a_string() {
    let mut engine = Engine::new();
    assert_eq!(
        engine.execute_bytes(b"newgame seed 42").lines,
        Engine::new().execute("newgame seed 42").lines
    );
}

#[test]
fn every_response_is_a_tag_followed_by_single_token_key_value_pairs() {
    // Drive every command, in both their working and failing forms, and hold
    // all of it to the one rule that makes the format extensible.
    let script = [
        "id",
        "state",
        "move left",
        "undo",
        "bogus",
        "newgame seed 42",
        &format!("setposition board {PACKED_LEFT} score 5 seed 9"),
        "move left",
        "move up",
        "state",
        "undo",
        "redo",
        "redo",
        "history",
        "replay game g1:q:-:A:A:",
        "replay game broken",
        "history",
        &format!("setposition board {LOCKED} seed 9"),
        "state",
        "move down",
    ];
    let script: Vec<&str> = script.iter().map(AsRef::as_ref).collect();

    let lines = run(&script);
    assert!(lines.len() >= script.len(), "every command answers");

    for line in &lines {
        let (tag, pairs) = parse_response(line);
        assert!(
            matches!(tag.as_str(), "ok" | "error" | "id" | "idok"),
            "unexpected response tag {tag} in: {line}"
        );
        for (key, value) in pairs {
            assert!(!key.is_empty() && !value.is_empty());
            assert!(
                !value.chars().any(char::is_whitespace),
                "value for {key} must be a single token: {line}"
            );
        }
    }
}

#[test]
fn a_game_played_to_the_end_stays_consistent() {
    let mut engine = Engine::new();
    engine.execute("newgame seed 2024");

    for _ in 0..10_000 {
        let state = engine.execute("state").lines.remove(0);
        if field(&state, "status") == "over" {
            assert_eq!(field(&state, "legal"), "none");
            for direction in ["up", "down", "left", "right"] {
                let line = engine.execute(&format!("move {direction}")).lines.remove(0);
                assert_eq!(
                    field(&line, "moved"),
                    "false",
                    "a finished game accepts nothing"
                );
            }
            return;
        }

        let direction = field(&state, "legal")
            .split(',')
            .next()
            .expect("a live game has a legal move")
            .to_owned();
        let played = engine.execute(&format!("move {direction}")).lines.remove(0);
        assert_eq!(field(&played, "moved"), "true", "legal listed {direction}");
    }

    panic!("a 4x4 game must end within 10000 moves");
}
