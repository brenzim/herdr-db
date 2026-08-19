//! Naming the Pane.
//!
//! The title is the only on-screen statement of which database the Client was connected to
//! and where that answer came from (ADR-0007), so setting it is one of the things the
//! binary does above the seam. This module states the argv herdr is asked for; issuing it
//! belongs to `main`, which is also the only place that knows it has not `exec`ed yet.

/// The herdr binary the Pane asks to name it.
pub const HERDR: &str = "herdr";

/// The variable herdr gives the Pane process, so a Pane can name itself with no plumbing
/// of its own.
pub const PANE_ID_VAR: &str = "HERDR_PANE_ID";

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
