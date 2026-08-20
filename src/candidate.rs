//! A Candidate: a DSN a Resolution Strategy proposes, together with where it came from.
//!
//! Held as the parts rather than as a finished string, because the title has to state some
//! of them — the database, the port, the role — and a title picked apart from a DSN would
//! be a parser of the plugin's own output.

/// Where a Candidate came from, as the title says it out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Docker,
    Compose,
}

impl Origin {
    /// The one word the title uses for this origin.
    ///
    /// `compose` is the rendered read, where Compose itself resolved the Stack. The
    /// approximate read that follows when the render will not run takes `compose~`, so the
    /// two are never confusable on screen and neither has to rename the other.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Compose => "compose",
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

/// The scheme a DSN is addressed with, held as its bare word: the whole prefix is a
/// credential-shaped literal, and `tests/redaction.rs` refuses one anywhere in `src/`.
const SCHEME: &str = "postgres";

/// The address a local Candidate is reached at. Numeric, never the name: a machine that
/// resolves the name to `::1` first fails against a port Docker bound on IPv4.
const HOST: &str = "127.0.0.1";

/// `value` with everything outside the unreserved set percent-encoded, byte by byte, for
/// the two places this encoding is applied: the role and the password. The database is left
/// legible in the path instead. A password of `p@ss/word` written in as it reads addresses
/// a different host and a different database, and the Client has no way to know it was
/// handed the wrong one.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            reserved => format!("%{reserved:02X}"),
        })
        .collect()
}

/// The characters that stop being part of a database name and start punctuating the DSN
/// around it: `/` re-roots the path, `?` and `#` end it, and `%` is what decides whether
/// any of the encoding here means anything at all.
const STRUCTURAL: [char; 4] = ['/', '?', '#', '%'];

/// `value` with only [`STRUCTURAL`], whitespace and control characters percent-encoded, for
/// the database name.
/// Encoding it the way the role and the password are encoded would be just as safe and
/// unreadable: the name is what the title states and what the user checks the Client against,
/// so everything that cannot move the boundary between the DSN's parts stays as written.
fn readable(value: &str) -> String {
    let mut written = String::with_capacity(value.len());
    for character in value.chars() {
        if !STRUCTURAL.contains(&character) && !character.is_whitespace() && !character.is_control()
        {
            written.push(character);
            continue;
        }
        let mut utf8 = [0_u8; 4];
        for byte in character.encode_utf8(&mut utf8).as_bytes() {
            written.push_str(&format!("%{byte:02X}"));
        }
    }
    written
}

impl Candidate {
    /// The DSN the Client is launched against.
    pub fn dsn(&self) -> String {
        let credentials = match &self.password {
            Some(password) => format!("{}:{}", encoded(&self.role), encoded(password)),
            None => encoded(&self.role),
        };
        format!(
            "{SCHEME}://{credentials}@{HOST}:{}/{}",
            self.port,
            readable(&self.database),
        )
    }

    /// The Pane title: which database is in use, where the answer came from, and which
    /// role it connected as — plus, when there were several, that this is the first of
    /// `of` Candidates.
    ///
    /// Never carries the password, and never the DSN it is assembled from the parts of:
    /// the title is on screen for as long as the Pane is, in front of whoever walks past
    /// (AC 10, ADR-0005).
    pub fn title(&self, of: usize) -> String {
        let mut title = format!(
            "{}@{} · {} · {}",
            self.database,
            self.port,
            self.origin.label(),
            self.role,
        );
        // Only when there was a choice to make, because a Pane that says `1 of 1` invites
        // the user to look for the other one. Always the first, because the Candidates are
        // ranked and the highest-ranked one is what launched.
        if of > 1 {
            title.push_str(&format!(" · 1 of {of}"));
        }
        title
    }
}
