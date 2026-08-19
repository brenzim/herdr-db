//! Naming the Pane.
//!
//! The title is the only on-screen statement of which database the Client was connected to
//! and where that answer came from (ADR-0007), so setting it is one of the things the
//! binary does above the seam. This module says which Pane there is to name and states the
//! argv herdr is asked for; issuing it belongs to `main`, which is also the only place that
//! knows it has not `exec`ed yet.

/// The herdr binary the Pane asks to name it.
pub const HERDR: &str = "herdr";

/// The variable herdr gives the Pane process, so a Pane can name itself with no plumbing
/// of its own.
pub const PANE_ID_VAR: &str = "HERDR_PANE_ID";

/// The Pane this process is running in, or `None` when herdr named none — this binary run
/// by hand, which is nothing to name rather than a fault. Read here rather than in `main`
/// so that the variable and the rule for reading it stay together, the way
/// `InvocationContext::from_env` keeps herdr's other variable.
///
/// An empty value has named no Pane, the same rule the invocation context's tiers and the
/// container's environment both follow: renaming `""` asks herdr about a Pane that is not
/// there.
pub fn id() -> Option<String> {
    std::env::var(PANE_ID_VAR).ok().filter(|id| !id.is_empty())
}

/// The argv, after the program, that sets the Pane `pane_id`'s durable label to `title`.
///
/// `rename` rather than the plugin-facing `report-metadata`, which rejects `--source` and
/// `--title` on herdr 0.8.0: the `label` this writes is a field separate from the terminal
/// title, which is what makes it survive the Client writing a title of its own. The id
/// comes first because herdr parses a flag only after it.
pub fn rename_args(pane_id: &str, title: &str) -> Vec<String> {
    vec![
        "pane".to_string(),
        "rename".to_string(),
        pane_id.to_string(),
        title.to_string(),
    ]
}
