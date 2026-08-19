//! A Candidate: a DSN a Resolution Strategy proposes, together with where it came from.
//!
//! Held as the parts rather than as a finished string, because the title has to state some
//! of them — the database, the port, the role — and a title picked apart from a DSN would
//! be a parser of the plugin's own output.

/// Where a Candidate came from, as the title says it out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Docker,
}

impl Origin {
    /// The one word the title uses for this origin.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
        }
    }
}

/// One proposed connection, with everything the Launch is rendered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub origin: Origin,
    pub database: String,
    pub role: String,
    pub password: Option<String>,
    pub port: u16,
    /// The container's name, without Docker's leading `/`. Carried for the tie-break when
    /// two Candidates publish the same port, and for nothing else.
    pub container: String,
    pub read_only: bool,
}

impl Candidate {
    /// The DSN the Client is launched against.
    //
    // A placeholder: what a Candidate renders to is the second half of #4. Until then a
    // Candidate is observable as a Launch existing at all, which is what matching decides.
    pub fn dsn(&self) -> String {
        String::new()
    }

    /// The Pane title: which database is in use, where the answer came from, and which
    /// role it connected as — plus which of `of` Candidates this one is.
    //
    // A placeholder, for the same reason as `dsn`.
    pub fn title(&self, _rank: usize, _of: usize) -> String {
        String::new()
    }
}
