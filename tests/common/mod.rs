//! What more than one test binary needs. Not a test target of its own — `tests/` compiles
//! only its top-level files, so this is reached with `mod common;`.

#![allow(dead_code)] // Each test binary uses the part of this it needs, not all of it.

use std::path::{Path, PathBuf};

/// The plugin's own directory, whatever a test's working directory happens to be.
pub fn plugin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every Rust source file the plugin is built from, with its text — read once, because the
/// guards that scan the source all want both.
///
/// Recursive on purpose: the Resolution Strategies these guards exist to police will arrive
/// in subdirectories of `src/`, and a walker that only reads the top level would stop
/// scanning them without ever failing.
pub fn sources() -> Vec<(PathBuf, String)> {
    let src = plugin_root().join("src");
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

fn collect(dir: &Path, found: &mut Vec<(PathBuf, String)>) {
    let entries = std::fs::read_dir(dir).expect("list the source directory");
    for entry in entries.flatten() {
        let path = entry.path();
        // The entry already knows what it is; `is_dir` would re-stat it, and would follow a
        // symlink out of the repository while it was at it.
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect(&path, found);
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            let text = std::fs::read_to_string(&path).expect("read the source file");
            found.push((path, text));
        }
    }
}
