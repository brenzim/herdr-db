//! The Pane body. herdr spawns this binary in a real PTY; it execs the Client in place, so
//! the Client *is* the Pane rather than something this process wraps (ADR-0001).
//!
//! Everything decided here is decided by [`plan`]: this file reads the invocation context,
//! hands it and the real world to the seam, and does what comes back.

use std::io::{BufRead, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use herdr_db::context::InvocationContext;
use herdr_db::diagnosis::{Turn, diagnosis_screen, on_input};
use herdr_db::host::RealHost;
use herdr_db::plan::{Diagnosis, Launch, Plan, plan};

fn main() -> ExitCode {
    // Read once: herdr fixed the invocation context when it spawned this Pane, so a retry
    // re-runs resolution against the same context. What changes between attempts is the
    // world behind the Host — the user starting the database they were told was missing.
    let context = InvocationContext::from_env();
    let host = RealHost;

    loop {
        match plan(&context, &host) {
            Plan::Launch(launch) => return launch_client(launch),
            // Only a retry leaves the screen up; anything else is the user closing the Pane
            // themselves, which is not a failure (AC 7).
            Plan::Decline(diagnosis) if wants_retry(&diagnosis) => continue,
            Plan::Decline(_) => return ExitCode::SUCCESS,
        }
    }
}

/// Shows `diagnosis` and waits for the user to say what to do about it: `true` if they
/// asked to run resolution again, `false` if they asked to close the Pane. Returns for
/// nothing else — an unrecognised key leaves the screen exactly as it is.
fn wants_retry(diagnosis: &Diagnosis) -> bool {
    print!("{}", diagnosis_screen(diagnosis));
    // A Pane's stdout is a PTY, so the prompt has to be flushed before the read that
    // follows it, or the user waits at a screen that has not been drawn.
    let _ = std::io::stdout().flush();

    let mut stdin = std::io::stdin().lock();
    let mut line = String::new();
    loop {
        line.clear();
        // A stdin that cannot be read is a stdin that will not be read again: treat it as
        // the EOF it effectively is rather than looping on the error for ever.
        if stdin.read_line(&mut line).is_err() {
            return false;
        }
        match on_input(&line) {
            Turn::Retry => return true,
            Turn::Quit => return false,
            // An unrecognised key is no reason to take the screen away, so keep waiting.
            Turn::Ignore => continue,
        }
    }
}

/// Becomes the Client. `exec` only returns on failure; on success this process *is* the
/// Client (ADR-0001). Every path here reports rather than panics — a panic in a Pane is a
/// crash the user watches happen (ADR-0004).
fn launch_client(launch: Launch) -> ExitCode {
    let failure = match launch.argv.split_first() {
        Some((program, args)) => format!(
            "could not launch {program}: {}",
            Command::new(program).args(args).exec()
        ),
        None => "the Client has no program to launch".to_string(),
    };

    eprintln!("herdr-db: {failure}");
    ExitCode::FAILURE
}
