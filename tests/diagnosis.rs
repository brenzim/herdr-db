//! The diagnosis Pane: what it draws, and what a key means.
//!
//! A `Decline` nobody can read is not demoable behaviour, so the Pane stays alive showing
//! why resolution declined and offers a documented key to run it again in place. The Pane's
//! terminal I/O — print the screen, read a line, loop — stays manually verified; the two
//! decisions worth testing are what the screen says and what a line of input means.

use std::path::PathBuf;

use herdr_db::diagnosis::{Turn, diagnosis_screen, on_input};
use herdr_db::plan::Diagnosis;

/// Every `Diagnosis` the plugin can show. Listed out so that a new fault has to be given a
/// screen of its own rather than inheriting one silently.
fn every_diagnosis() -> Vec<Diagnosis> {
    vec![
        Diagnosis::ContextMissing,
        Diagnosis::ContextEmpty,
        Diagnosis::ContextUnreadable,
        Diagnosis::NoProjectIdentified,
        Diagnosis::NoConnectionFound {
            project: PathBuf::from("/Users/b/AI/herdr-db"),
        },
    ]
}

#[test]
fn the_screen_says_why_resolution_declined() {
    // The whole reason the Pane stays open: it has to carry the diagnosis itself, not a
    // generic failure the user then has to go and interpret somewhere else.
    for diagnosis in every_diagnosis() {
        let screen = diagnosis_screen(&diagnosis);
        assert!(
            screen.contains(&diagnosis.message()),
            "the screen for {diagnosis:?} does not say why it declined. It said: {screen}",
        );
    }
}

#[test]
fn the_screen_documents_the_keys_it_accepts() {
    // AC 8 says a *documented* keypress re-runs resolution. The documentation the user can
    // actually reach is the screen in front of them, and it must name Enter too — input is
    // line-based, so a bare `r` does nothing until the line is submitted.
    for diagnosis in every_diagnosis() {
        let screen = diagnosis_screen(&diagnosis);
        for expected in ["r", "q", "Enter"] {
            assert!(
                screen.contains(expected),
                "the screen for {diagnosis:?} never names `{expected}`, so the key that \
                 retries is undocumented where the user is looking. It said: {screen}",
            );
        }
    }
}

#[test]
fn the_documented_retry_key_runs_resolution_again() {
    assert_eq!(on_input("r\n"), Turn::Retry);
}

#[test]
fn the_documented_quit_key_closes_the_pane() {
    assert_eq!(on_input("q\n"), Turn::Quit);
}

#[test]
fn a_key_is_taken_however_the_user_typed_it() {
    // A line read from a terminal carries its newline, and a user who typed a capital or
    // hit space first meant the same key. Refusing those reads as a Pane ignoring input.
    for line in ["r", "r\n", "R\n", " r \r\n"] {
        assert_eq!(on_input(line), Turn::Retry, "input {line:?} did not retry");
    }
    for line in ["q", "q\n", "Q\n", " q \r\n"] {
        assert_eq!(on_input(line), Turn::Quit, "input {line:?} did not quit");
    }
}

#[test]
fn a_closed_stdin_closes_the_pane() {
    // EOF is the empty read: `read_line` on a closed stdin returns `Ok(0)` and writes
    // nothing, and it does so for ever. Anything but Quit here spins the loop and burns a
    // core in a Pane the user is looking at.
    assert_eq!(on_input(""), Turn::Quit);
}

#[test]
fn a_bare_enter_leaves_the_pane_as_it_is() {
    // "\n" is a user who submitted an empty line, not a stdin that has closed. The two read
    // almost alike and mean opposite things, so a Pane that conflates them shuts itself the
    // first time the user leans on Enter.
    assert_eq!(on_input("\n"), Turn::Ignore);
}

#[test]
fn an_unrecognised_key_leaves_the_pane_alive() {
    // AC 7: on a Decline the Pane remains alive displaying the diagnosis. A key it does not
    // know is not a reason to take that screen away.
    for line in ["x\n", "quit\n", "retry\n", "  \n"] {
        assert_eq!(
            on_input(line),
            Turn::Ignore,
            "input {line:?} was not ignored"
        );
    }
}

/// AC 7, guarded in the source because the Pane's loop — print, read a line, loop — is the
/// part that stays manually verified. The walking skeleton's `main` ended *every* path with
/// `std::process::exit(1)`; a Decline that kept taking it would close the Pane on the very
/// screen the user needs to read. `main` returns an exit code instead, so no path can
/// terminate the process out from under the loop.
#[test]
fn the_pane_binary_never_terminates_itself_mid_flight() {
    let main = include_str!("../src/main.rs");
    assert!(
        !main.contains("::exit("),
        "`src/main.rs` terminates the process directly. On a Decline that closes the Pane \
         on the diagnosis the user is meant to read (AC 7) — return an `ExitCode` from \
         `main` instead.",
    );
}
