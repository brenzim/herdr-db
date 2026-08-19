//! The Host: everything outside the process that Connection Resolution may consult.
//!
//! Injected as a port so that a Resolution Strategy can be driven from a test with neither
//! Docker nor a live PostgreSQL server. Every method degrades rather than fails — `None`,
//! or an empty `Vec` — because a Decline is classified from what is *missing*, never from
//! the text of an io error, and because a panic in a Pane is a crash the user watches
//! happen (ADR-0004).

use std::path::{Path, PathBuf};

/// What a command said. Held as owned `String`s because a Strategy reads them, and a test
/// double has to be able to make one up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The world, as Connection Resolution is allowed to see it.
///
/// Deliberately no working directory: the Project is identified from herdr's invocation
/// context and never from the process working directory, which for a plugin Pane is the
/// plugin's own install directory (ADR-0004). A cwd here would make that mistake reachable.
pub trait Host {
    /// The contents of `path`, or `None` if it cannot be read for any reason.
    fn read_file(&self, path: &Path) -> Option<String>;

    /// The entries directly inside `path`, or an empty `Vec` if it cannot be listed.
    fn list_dir(&self, path: &Path) -> Vec<PathBuf>;

    /// Runs `program` to completion in `cwd`, or `None` if it could not be run at all. A
    /// command that ran and failed is `Some`, with its status — that is an answer, not an
    /// absence.
    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Option<Output>;
}

/// The real world. The only implementation the Pane binary uses, and the only place in the
/// crate that touches the filesystem or spawns a process.
pub struct RealHost;

impl Host for RealHost {
    fn read_file(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn list_dir(&self, path: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(path) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect()
    }

    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Option<Output> {
        let out = std::process::Command::new(program)
            .args(args)
            .current_dir(cwd)
            .output()
            .ok()?;
        Some(Output {
            // A command killed by a signal reports no code; -1 says "it did not finish"
            // without anyone having to look at signal numbers.
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}
