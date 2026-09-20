//! Drives the [`Engine`] over stdin and stdout.
//!
//! All the logic lives in the library; this is only the pipe.

use std::io::{self, BufRead, Write};

use game2048::Engine;

fn main() -> io::Result<()> {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut engine = Engine::new();

    for line in stdin.lines() {
        let response = engine.execute(&line?);
        for output in &response.lines {
            writeln!(stdout, "{output}")?;
        }
        stdout.flush()?;

        if response.exit {
            break;
        }
    }

    Ok(())
}
