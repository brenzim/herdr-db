//! AC 9: no raw DSN appears in any log output or on screen.
//!
//! A DSN carries real credentials, and ADR-0005 records that every DSN is redacted wherever
//! it is displayed or logged — "a future debugging change that logs a raw DSN is a security
//! regression". This is that regression, caught in source.
//!
//! The guard is written before the first DSN exists, which is exactly when a guard of this
//! kind is worth having: no Resolution Strategy has been built yet, so the only DSN the
//! plugin ever held was the walking skeleton's hardcoded one, and it is gone. Scanning all
//! of `src/` with no exemption — `src/client.rs` included, which takes a `dsn: &str` and
//! holds no literal of its own — is a stronger statement than a scan with carve-outs, and
//! there is less of it to explain. Written in the idiom `tests/launcher.rs` already uses to
//! keep the README from recommending what the launcher refuses to do.

use std::path::{Path, PathBuf};

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
    for file in sources() {
        let text = std::fs::read_to_string(&file).expect("read the source file");
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
    for file in sources() {
        let text = std::fs::read_to_string(&file).expect("read the source file");
        for (number, line) in text.lines().enumerate() {
            // Prose is allowed to discuss the DSN; only code that shows one is the fault.
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Two ways to show a secret: interpolate it by name anywhere, or hand it to
            // something that displays. Either is enough on its own.
            let interpolated = SECRET_BEARING
                .iter()
                .find(|name| line.contains(&format!("{{{name}")));
            let displayed = DISPLAY_MACROS
                .iter()
                .any(|display| line.contains(display))
                .then(|| SECRET_BEARING.iter().find(|name| line.contains(*name)))
                .flatten();
            let shown = interpolated.or(displayed);
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

/// Every Rust source file the plugin is built from.
fn sources() -> Vec<PathBuf> {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    collect(&src, &mut found);
    assert!(
        found.len() > 1,
        "found {} source files under {} — the scan is looking in the wrong place, and a \
         guard that scans nothing passes for ever",
        found.len(),
        src.display(),
    );
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("list the source directory");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found);
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            found.push(path);
        }
    }
}
