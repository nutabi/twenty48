//! Drives the [`Engine`] over stdin and stdout.
//!
//! All the logic lives in the library; this is only the pipe. Lines are read
//! as bytes rather than through [`BufRead::lines`], because that yields an
//! error for input that is not valid UTF-8 — which would end the session, and
//! the game in progress with it, over a single bad byte.

use std::io::{self, BufRead, Write};

use game2048::Engine;

fn main() -> io::Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut engine = Engine::new();
    let mut line = Vec::new();

    loop {
        line.clear();
        if stdin.read_until(b'\n', &mut line)? == 0 {
            break;
        }

        while let Some(b'\n' | b'\r') = line.last() {
            line.pop();
        }

        let response = engine.execute_bytes(&line);
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
