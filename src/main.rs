//! The Pane body. herdr spawns this binary in a real PTY; it execs the Client in place, so
//! the Client *is* the Pane rather than something this process wraps (ADR-0001).

use std::os::unix::process::CommandExt;
use std::process::Command;

use herdr_db::client;

/// Walking skeleton: Connection Resolution does not exist yet, so the DSN is hardcoded.
/// Once it exists, `main` is handed a fully-formed launch instead of naming a DSN at all.
const DSN: &str = "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable";

fn main() {
    let argv = client::argv(DSN);

    // `exec` only returns on failure; on success this process has become the Client. Every
    // other path here reports rather than panics — a panic in a Pane is a crash the user
    // watches happen (ADR-0004).
    let failure = match argv.split_first() {
        Some((program, args)) => format!(
            "could not launch {program}: {}",
            Command::new(program).args(args).exec()
        ),
        None => "the Client has no program to launch".to_string(),
    };

    eprintln!("herdr-db: {failure}");
    std::process::exit(1);
}
