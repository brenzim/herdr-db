//! AC 9: no raw DSN appears in any log output or on screen.
//!
//! A DSN carries real credentials, and ADR-0005 records that every DSN is redacted wherever
//! it is displayed or logged — "a future debugging change that logs a raw DSN is a security
//! regression". This is that regression, caught in source.
//!
//! All of `src/` is scanned with no exemption, `src/client.rs` included.

mod common;

use common::sources;

/// The URL schemes a PostgreSQL DSN is written with. A literal in either form is a
/// credential someone has typed into the repository.
const SCHEMES: [&str; 2] = ["postgres://", "postgresql://"];

/// Everything that puts text in front of the user or into a log.
const DISPLAY_MACROS: [&str; 8] = [
    "print!",
    "println!",
    "eprint!",
    "eprintln!",
    "format!",
    "write!",
    "writeln!",
    "panic!",
];

/// What a DSN travels as: itself, or the argv it is a positional argument of (ADR-0002 —
/// the Client takes the DSN as argv, so an argv shown whole shows the credentials).
const SECRET_BEARING: [&str; 2] = ["dsn", "argv"];

#[test]
fn no_source_file_carries_a_dsn_literal() {
    for (file, text) in sources() {
        for scheme in SCHEMES {
            assert!(
                !text.contains(scheme),
                "{} contains a `{scheme}` literal. A DSN belongs to Connection Resolution \
                 at run time and never to the source: a hardcoded one connects the user to \
                 a database nobody resolved, and a real one is a credential in the \
                 repository.",
                file.display(),
            );
        }
    }
}

#[test]
fn no_source_file_displays_a_dsn() {
    let interpolated = SECRET_BEARING.map(|name| format!("{{{name}"));

    for (file, text) in sources() {
        for (number, line) in text.lines().enumerate() {
            // Prose is allowed to discuss the DSN; only code that shows one is the fault.
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Two ways to show a secret: interpolate it by name anywhere, or name it on a
            // line that displays. Either is enough on its own.
            let displays = DISPLAY_MACROS.iter().any(|display| line.contains(display));
            let shown = SECRET_BEARING
                .iter()
                .zip(&interpolated)
                .find(|(name, form)| {
                    line.contains(form.as_str()) || (displays && line.contains(*name))
                })
                .map(|(name, _)| name);
            assert!(
                shown.is_none(),
                "{}:{} shows a `{}` to the user or to a log. Every DSN is redacted wherever \
                 it is displayed or logged (AC 9, ADR-0005). The line reads: {}",
                file.display(),
                number + 1,
                shown.copied().unwrap_or_default(),
                line.trim(),
            );
        }
    }
}
