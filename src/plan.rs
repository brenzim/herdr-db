//! The seam. `plan()` is the one boundary the plugin is tested at: given what herdr said
//! and a view of the world, it either launches the Client or declines readably.

use std::path::PathBuf;

use crate::client;
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
    NoConnectionFound { project: PathBuf },
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
    let candidates = docker::candidates(&project, host);
    let of = candidates.len();
    // A Strategy that found nothing is not a fault of its own: Docker being absent looks
    // from here exactly like Docker running nothing, and both leave the chain to go on.
    let Some(candidate) = candidates.into_iter().next() else {
        return Plan::Decline(Diagnosis::NoConnectionFound { project });
    };
    Plan::Launch(Launch {
        argv: client::argv(&candidate.dsn()),
        title: candidate.title(1, of),
        read_only: candidate.read_only,
    })
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
            Self::NoConnectionFound { project } => format!(
                "no database connection was found for the Project at {}.",
                project.display(),
            ),
        }
    }
}
