//! herdr-db — a herdr plugin that opens a PostgreSQL browser in a Pane, already connected
//! to the database belonging to the Project that the Worktree herdr was invoked from
//! identifies (ADR-0003).

pub mod candidate;
pub mod client;
pub mod context;
pub mod diagnosis;
pub mod docker;
pub mod host;
pub mod pane;
pub mod plan;
