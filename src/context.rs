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

/// herdr's context payload, parsed. Held as the JSON tree rather than as a struct of
/// concrete fields: herdr's schema makes every property nullable, and a plugin that must
/// survive a herdr that changed (ADR-0004) cannot let one property of an unexpected *type*
/// abort the whole parse. Deserializing into concrete fields would do exactly that — a
/// `focused_pane_cwd` that arrived as an object would throw away a perfectly good
/// `workspace_cwd` and report the context as unreadable JSON when it plainly is not.
pub(crate) struct RawContext {
    payload: serde_json::Value,
}

impl RawContext {
    /// The payload as a context, or `None` if it is not a JSON object — which is the one
    /// shape that is not a context herdr could have sent, however well-formed it is.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let payload: serde_json::Value = serde_json::from_str(raw).ok()?;
        payload.is_object().then_some(Self { payload })
    }

    /// The Project this context identifies: the Worktree's repository root, falling back
    /// to the focused Pane's cwd, then the workspace's (ADR-0004). The process working
    /// directory is not a tier and never will be — for a plugin Pane it is the plugin's own
    /// install directory, which would resolve every Project to the plugin itself.
    ///
    /// An empty value is a tier that said nothing, not a path: `find` therefore tests the
    /// tiers one at a time, and a tier that says nothing cannot settle the chain. Filtering
    /// empties once, after the chain has settled, would root the Project at `/`. A tier
    /// carrying something that is not a string has said nothing either, and falls through
    /// the same way rather than taking the whole context down with it.
    pub(crate) fn project(&self) -> Option<PathBuf> {
        [
            self.payload
                .get("worktree")
                .and_then(|worktree| worktree.get("repo_root")),
            self.payload.get("focused_pane_cwd"),
            self.payload.get("workspace_cwd"),
        ]
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .find(|path| !path.is_empty())
        .map(PathBuf::from)
    }
}
