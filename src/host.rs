//! The Host: everything outside the process that Connection Resolution may consult.
//!
//! Injected as a port so that a Resolution Strategy can be driven from a test with neither
//! Docker nor a live PostgreSQL server. Every method degrades rather than fails — `None`,
//! or an empty `Vec` — because a Decline is classified from what is *missing*, never from
//! the text of an io error, and because a panic in a Pane is a crash the user watches
//! happen (ADR-0004).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a command may take before the Host stops waiting for it.
///
/// A half-dead Docker Desktop — a far more common state than a cleanly stopped one — answers
/// `docker` by never answering, and a Pane that draws nothing is worse than one that
/// Declines. Long enough that a cold but healthy Docker still finishes first, short enough
/// that a user who is watching does not conclude the plugin is broken.
pub const DEADLINE: Duration = Duration::from_secs(10);

/// How often the wait looks to see whether the command has finished. Small against
/// `DEADLINE`, so expiry is punctual, and large enough that the wait costs nothing.
const POLL: Duration = Duration::from_millis(10);

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

    /// Runs `program` in `cwd`, or `None` if it could not be run at all *or* did not finish
    /// in time. A command that ran and failed is `Some`, with its status — that is an
    /// answer, not an absence.
    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Option<Output>;

    /// `path` with every symlink resolved, or `None` if it cannot be resolved at all.
    /// Deliberately no default body: one that read the real filesystem would put that
    /// access outside `RealHost`, and one that answered `None` would let a Host silently
    /// skip a comparison that only holds once both sides are canonical.
    fn canonicalize(&self, path: &Path) -> Option<PathBuf>;
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
        self.run_within(program, args, cwd, DEADLINE)
    }

    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }
}

impl RealHost {
    /// `Host::run` under the deadline given rather than `DEADLINE`. Exists so that a test can
    /// prove expiry in milliseconds instead of paying the real deadline in wall clock on
    /// every run of the suite; the Pane always goes through `run`.
    pub fn run_within(
        &self,
        program: &str,
        args: &[&str],
        cwd: &Path,
        deadline: Duration,
    ) -> Option<Output> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;

        // Both pipes are drained on their own threads for the whole time the command runs. A
        // wait that read them afterwards would deadlock on any command saying more than a
        // pipe buffer holds — and a rendered Compose Stack says far more than that.
        let said = drain(child.stdout.take()?);
        let complained = drain(child.stderr.take()?);

        let expires = Instant::now() + deadline;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(_) => return None,
            }
            if Instant::now() >= expires {
                // The reader threads are left to end on their own once the pipes close: a
                // command sick enough to outlive its deadline may have handed a pipe to a
                // child of its own, and joining on that would be the hang this deadline
                // exists to prevent.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            std::thread::sleep(POLL);
        };

        Some(Output {
            // A command killed by a signal reports no code; -1 says "it did not finish"
            // without anyone having to look at signal numbers.
            status: status.code().unwrap_or(-1),
            stdout: said.join().ok()?,
            stderr: complained.join().ok()?,
        })
    }
}

/// Reads one of a command's pipes to its end on a thread of its own.
fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut said = Vec::new();
        let _ = pipe.read_to_end(&mut said);
        // The buffer is reused where it can be: a rendered Compose Stack is the hundreds of
        // kilobytes the drain above exists for, and `from_utf8_lossy` on valid UTF-8 borrows
        // it only for `into_owned` to allocate and copy the whole thing a second time. A
        // command that answers in something other than UTF-8 still degrades rather than
        // fails, which is the whole rule this Host is written to.
        String::from_utf8(said)
            .unwrap_or_else(|said| String::from_utf8_lossy(said.as_bytes()).into_owned())
    })
}
