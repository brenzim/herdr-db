//! herdr's invocation context: the raw payload, carried across the seam unparsed.
//!
//! Parsing happens *below* `plan()`, not above it, so that a malformed context is a
//! `Plan` the tests can observe rather than a failure that happened before the seam.

use std::path::PathBuf;

/// The invocation context exactly as herdr handed it over.
pub struct InvocationContext {
    /// The raw value of `HERDR_PLUGIN_CONTEXT_JSON`; `None` when the variable is unset.
    raw: Option<String>,
}

impl InvocationContext {
    /// The context this process was actually invoked with. Above the seam: this is the one
    /// function here that touches the environment, and nothing tests it.
    pub fn from_env() -> Self {
        Self {
            raw: std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok(),
        }
    }

    /// The context a test states outright.
    pub fn from_json(raw: Option<&str>) -> Self {
        Self {
            raw: raw.map(str::to_string),
        }
    }

    /// The payload, for the parser below the seam.
    pub(crate) fn raw(&self) -> Option<&str> {
        self.raw.as_deref()
    }
}

/// herdr's context payload, parsed. Every field is optional and unknown fields are ignored,
/// because herdr's schema makes every property nullable and this plugin must survive a herdr
/// that grew a property it has never heard of (ADR-0004).
#[derive(serde::Deserialize)]
pub(crate) struct RawContext {
    worktree: Option<RawWorktree>,
    focused_pane_cwd: Option<String>,
    workspace_cwd: Option<String>,
}

/// The `worktree` object, when herdr was invoked from one.
#[derive(serde::Deserialize)]
pub(crate) struct RawWorktree {
    repo_root: Option<String>,
}

impl RawContext {
    /// The payload as a structure, or `None` if it is not JSON at all.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }

    /// The Project this context identifies: the Worktree's repository root, falling back
    /// to the focused Pane's cwd, then the workspace's (ADR-0004). The process working
    /// directory is not a tier and never will be — for a plugin Pane it is the plugin's own
    /// install directory, which would resolve every Project to the plugin itself.
    ///
    /// An empty value is a tier that said nothing, not a path: `find` therefore tests the
    /// tiers one at a time, and a tier that says nothing cannot settle the chain. Filtering
    /// empties once, after the chain has settled, would root the Project at `/`.
    pub(crate) fn project(&self) -> Option<PathBuf> {
        [
            self.worktree
                .as_ref()
                .and_then(|worktree| worktree.repo_root.as_deref()),
            self.focused_pane_cwd.as_deref(),
            self.workspace_cwd.as_deref(),
        ]
        .into_iter()
        .flatten()
        .find(|path| !path.is_empty())
        .map(PathBuf::from)
    }
}
