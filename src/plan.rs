//! The seam. `plan()` is the one boundary the plugin is tested at: given what herdr said
//! and a view of the world, it either launches the Client or declines readably.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::candidate::Candidate;
use crate::client;
use crate::compose::{self, Stopped};
use crate::context::{InvocationContext, RawContext};
use crate::docker;
use crate::host::Host;

/// The outcome of Connection Resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Launch(Launch),
    Decline(Diagnosis),
}

/// A fully-formed launch: `main` runs this and names no DSN of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub argv: Vec<String>,
    pub title: String,
    pub read_only: bool,
}

/// Why resolution declined, in the user's terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnosis {
    ContextMissing,
    ContextEmpty,
    ContextUnreadable,
    NoProjectIdentified,
    /// The Project declares databases and nothing is running any of them — the state a cold
    /// machine is in, and the only Decline the user can act on without leaving the Pane.
    DeclaredButNotRunning {
        stopped: Vec<Stopped>,
    },
    NoConnectionFound {
        project: PathBuf,
    },
}

/// Turns what herdr said, plus the world, into a Plan. Never panics: every fault is a
/// `Decline` the Pane can show (ADR-0004).
pub fn plan(context: &InvocationContext, host: &dyn Host) -> Plan {
    let Some(raw) = context.raw() else {
        return Plan::Decline(Diagnosis::ContextMissing);
    };
    // Before the parse, deliberately: serde rejects an empty payload and a malformed one
    // identically, and the user needs to be told which of the two happened.
    if raw.trim().is_empty() {
        return Plan::Decline(Diagnosis::ContextEmpty);
    }
    let Some(parsed) = RawContext::parse(raw) else {
        return Plan::Decline(Diagnosis::ContextUnreadable);
    };
    let Some(project) = parsed.project() else {
        return Plan::Decline(Diagnosis::NoProjectIdentified);
    };
    let sweep = docker::sweep(&project, host);
    let rendered = compose::candidates(&project, host, &sweep);
    let mut candidates = docker::candidates(&project, host, &sweep);
    candidates.extend(rendered.candidates);
    let candidates = ranked(candidates);
    let of = candidates.len();
    // A Strategy that found nothing is not a fault of its own: Docker being absent looks
    // from here exactly like Docker running nothing, and both leave the chain to go on.
    let Some(candidate) = candidates.into_iter().next() else {
        // Only here, with nothing resolved at all: a Project with one Stack up and another
        // down opens the one that is up and says nothing about the other, because something
        // to work in beats a diagnosis about something else.
        if !rendered.stopped.is_empty() {
            return Plan::Decline(Diagnosis::DeclaredButNotRunning {
                stopped: rendered.stopped,
            });
        }
        return Plan::Decline(Diagnosis::NoConnectionFound { project });
    };
    Plan::Launch(Launch {
        argv: client::argv(&candidate.dsn()),
        title: candidate.title(of),
        read_only: candidate.read_only,
    })
}

/// Every Strategy's Candidates as one list: ordered by the chain, then deduplicated so that
/// one database is one Candidate however many Strategies vouched for it.
///
/// The one place Candidate order is decided. A Strategy hands its Candidates over in
/// whatever order it read them — `docker ps` lists in whichever order it lists — and the
/// rank the title states must not be that order: only consistent behaviour is learnable, so
/// a Project resolves to the same Candidate every run. A Strategy sorting its own list first
/// could only sort it on a prefix of this key, which is work that cannot change the answer.
///
/// Sorted by origin rank *before* the port, because the chain is what the plugin believes
/// about which answer is most likely right, and a global sort by port would discard it — a
/// declared database on 5433 is not a better answer than a running one on 5434.
///
/// Deduplicated afterwards, on the container id: sorted first, so "the one that survives" is
/// "the highest-ranked one" and not "whichever Strategy happened to run first".
fn ranked(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates.sort_by(|one, other| {
        (one.origin.rank(), one.port, &one.container).cmp(&(
            other.origin.rank(),
            other.port,
            &other.container,
        ))
    });
    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.id.clone()));
    candidates
}

/// One line per stopped database, each of them the whole remedy for that one database.
///
/// The remedy is a Compose command and not the `start-db` action, which does not exist
/// until #12: a diagnosis pointing at something the user cannot invoke is worse than one
/// with no remedy at all. It names the *directory* rather than passing the file with `-f`,
/// because a `-f` drops the `docker-compose.override.yml` Compose loads beside it and would
/// start the Stack on a port the render never saw. And it names the service explicitly,
/// which is both what makes the line about one database and what activates the profile of a
/// service behind `profiles:`.
fn listed(stopped: &[Stopped]) -> String {
    stopped
        .iter()
        .map(|it| {
            format!(
                "\n  {} — service `{}`, declared in {} — start it with: \
                 cd {} && docker compose up -d {}",
                it.container,
                it.service,
                it.file.display(),
                quoted(&it.directory),
                it.service,
            )
        })
        .collect()
}

/// A path as a shell reads it back as one word. The remedy is written to be pasted, and a
/// Project at `/Users/b/My Projects/orders` otherwise offers a `cd` with two arguments —
/// which either fails or quietly lands somewhere else, and the `docker compose up` after
/// the `&&` then runs there. Single quotes, because they are the one quoting a shell does
/// not look inside; a single quote in the path closes them, escapes itself and reopens.
fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

impl Diagnosis {
    /// What to tell the user. Each fault reads differently because each has a different
    /// remedy: a herdr that said nothing is not a herdr that said something broken, and
    /// neither is a Project with no connection behind it.
    pub fn message(&self) -> String {
        match self {
            Self::ContextMissing => "herdr did not say where it was invoked from: \
                 HERDR_PLUGIN_CONTEXT_JSON is not set. Open this Pane from herdr rather \
                 than running the binary directly."
                .to_string(),
            Self::ContextEmpty => "herdr said where it was invoked from, but said nothing: \
                 HERDR_PLUGIN_CONTEXT_JSON is empty."
                .to_string(),
            Self::ContextUnreadable => "herdr's invocation context could not be read — \
                 HERDR_PLUGIN_CONTEXT_JSON is not valid JSON."
                .to_string(),
            Self::NoProjectIdentified => "herdr named no Worktree, no focused Pane and no \
                 workspace, so there is no Project to resolve a database for."
                .to_string(),
            Self::DeclaredButNotRunning { stopped } => {
                let count = stopped.len();
                let opening = if count == 1 {
                    "this Project declares a database that is not running:".to_string()
                } else {
                    format!("this Project declares {count} databases and none is running:")
                };
                format!("{opening}{}", listed(stopped))
            }
            Self::NoConnectionFound { project } => format!(
                "no database connection was found for the Project at {}.",
                project.display(),
            ),
        }
    }
}
