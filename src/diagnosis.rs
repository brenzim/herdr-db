//! The diagnosis Pane: what it draws, and what a key means.
//!
//! A `Decline` nobody can read is not demoable behaviour, so on a Decline the Pane stays
//! alive showing why and offers a key that runs resolution again in place. Only the two
//! decisions worth testing live here — the screen's text and a line of input's meaning.
//! The loop around them (print, read a line, repeat) is `main`'s, and is verified by hand.
//!
//! Input is line-based rather than raw-mode: the screen shows one paragraph, and a raw-mode
//! setup and teardown that has to survive `exec` and panics buys nothing for it. The cost is
//! that every key needs Enter after it, which is why the screen says so.

use crate::plan::Diagnosis;

/// The key that runs resolution again, as the screen documents it and `on_input` accepts it.
pub const RETRY_KEY: char = 'r';

/// The key that closes the Pane.
pub const QUIT_KEY: char = 'q';

/// What one line of input asks the Pane to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Run resolution again with the same invocation context, and show the new outcome.
    Retry,
    /// Close the Pane. The user asked, so this is an ordinary close and not a failure.
    Quit,
    /// Nothing the Pane knows about; it stays exactly as it is.
    Ignore,
}

/// The exact text the Pane draws while showing `d`, including the keys it accepts.
///
/// The diagnosis is the point of the screen and the keys are how the user acts on it, so
/// both are here: the only documentation reachable from inside a Pane is the Pane itself.
pub fn diagnosis_screen(d: &Diagnosis) -> String {
    format!(
        "herdr-db could not open a database Pane.\n\n\
         {}\n\n\
         [{RETRY_KEY}] then Enter — try again\n\
         [{QUIT_KEY}] then Enter — close this Pane\n",
        d.message(),
    )
}

/// What one line of input means.
///
/// Retry runs `plan()` again with the *same* invocation context: herdr fixed that for the
/// life of the process, and what changes between attempts is the world behind the `Host` —
/// the user starting the database they were told was missing.
pub fn on_input(line: &str) -> Turn {
    // EOF, and only EOF: `read_line` on a closed stdin writes nothing and returns `Ok(0)`,
    // for ever. Ignoring that spins the loop and burns a core in a Pane the user is looking
    // at. A user who merely pressed Enter submitted "\n", which is a different thing and
    // must not close their Pane.
    if line.is_empty() {
        return Turn::Quit;
    }
    // One key and nothing else, however it was typed: a line carries its newline, and a
    // capital or a stray space is the same keypress. `quit` spelled out is not.
    let key = line.trim().to_ascii_lowercase();
    let mut key = key.chars();
    match (key.next(), key.next()) {
        (Some(RETRY_KEY), None) => Turn::Retry,
        (Some(QUIT_KEY), None) => Turn::Quit,
        // Including an empty line: the user said nothing, so nothing happens (AC 7 — the
        // Pane stays alive on a Decline, and an unknown key is no reason to take it away).
        _ => Turn::Ignore,
    }
}
