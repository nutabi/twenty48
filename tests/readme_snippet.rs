//! Compiles the snippet printed in README.md, so the README cannot rot.

#[test]
fn readme_library_example() {
    use game2048::{Direction, Game, notation};

    let mut game = Game::with_seed(42);
    game.step(Direction::Left);

    let encoded = notation::encode(&game);
    let replayed = notation::decode(&encoded).expect("round trip");
    assert_eq!(replayed.board(), game.board());
}
