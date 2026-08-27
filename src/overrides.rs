//! Overrides: what the user says about a Project's connection, which no Strategy may
//! contradict.
//!
//! An Override is not a Resolution Strategy and yields no Candidate. A Candidate is built
//! from parts and addressed at localhost (`candidate.rs`), whereas an Override is an opaque
//! string the user wrote — the one route to a database that is not local (ADR-0005) — and
//! carrying it as a Candidate would mean parsing the user's own credential string back into
//! parts to rebuild it.

use std::path::{Path, PathBuf};

use crate::host::Host;

/// The one file Overrides are read from, inside herdr's plugin config directory.
const FILE: &str = "overrides.toml";

/// The table every Override is nested under, keyed by the Project's absolute path.
///
/// Nested rather than at the top level so that the file has somewhere to grow: ADR-0005
/// anticipates a repo-local source of the same format and several named connections per
/// Project, and a shape those cannot be added to is a breaking change to a file users have
/// hand-written.
const PROJECTS: &str = "projects";

/// Where the machine-local Override file is looked for.
pub struct Overrides {
    /// herdr's plugin config directory, or `None` when herdr named none.
    directory: Option<PathBuf>,
}

/// What the Override file says about one Project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overridden {
    /// No config directory, no file the plugin could read, or a file naming no Override for
    /// this Project — a file with no `projects` table at all included.
    Silent,
    Pinned(Override),
    /// The file was read and is not TOML. Carries the path and nothing out of the file: a
    /// parser's own error quotes the line it failed on, which here is the line the
    /// credentials are written on (ADR-0005).
    Unreadable {
        file: PathBuf,
    },
    /// The file parses and names this Project, and the entry carries no connection — a
    /// misspelled or missing `dsn`/`label`, or an entry that is not a table at all. Carries
    /// the path only, on the same ground as `Unreadable`: the entry holds the DSN whatever
    /// else it is missing.
    Incomplete {
        file: PathBuf,
    },
}

/// One connection a user pinned to a Project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub dsn: String,
    pub label: String,
    pub read_only: bool,
}

impl Overrides {
    /// Where herdr said to look. Above the seam: this is the one function here that touches
    /// the environment, and nothing tests it.
    pub fn from_env() -> Self {
        Self::at(
            std::env::var("HERDR_PLUGIN_CONFIG_DIR")
                .ok()
                .as_deref()
                .map(Path::new),
        )
    }

    /// The directory a test states outright.
    ///
    /// An empty value has named no directory, the same rule the invocation context's tiers
    /// and the Pane id are read under. It cannot be allowed through as a path: joining the file name
    /// onto it yields a *relative* `overrides.toml`, which resolves against the process
    /// working directory — for a plugin Pane, this plugin's own install directory (ADR-0004).
    pub fn at(directory: Option<&Path>) -> Self {
        Self {
            directory: directory
                .filter(|directory| !directory.as_os_str().is_empty())
                .map(Path::to_path_buf),
        }
    }

    /// What the file pins `project` to, or `Silent` if it pins nothing.
    pub(crate) fn pinning(&self, project: &Path, host: &dyn Host) -> Overridden {
        let Some(directory) = &self.directory else {
            return Overridden::Silent;
        };
        let file = directory.join(FILE);
        // A file that cannot be read at all is not a fault: `Host::read_file` answers
        // `None` for any reason, so an absent file — the ordinary state of a machine with
        // no Override on it — and one that could not be opened arrive here identically, and
        // there is nothing to tell the user apart from the two. Only bytes that were read
        // and are not TOML is something they can go and fix.
        let Some(raw) = host.read_file(&file) else {
            return Overridden::Silent;
        };
        let Ok(parsed) = toml::from_str::<toml::Value>(&raw) else {
            return Overridden::Unreadable { file };
        };
        let Some(projects) = parsed.get(PROJECTS).and_then(toml::Value::as_table) else {
            return Overridden::Silent;
        };
        // An entry naming this Project is the user saying they pinned it, so from here the
        // file either yields a connection or the Project declines: falling back to the chain
        // would resolve a Candidate around an Override the user believes is in force.
        let Some(entry) = keyed(projects, project, host) else {
            return Overridden::Silent;
        };
        stated(entry).map_or(Overridden::Incomplete { file }, Overridden::Pinned)
    }
}

/// The entry of `projects` naming `project`, matched as a path and never as text: written
/// with the trailing slash a shell's completion leaves behind, a key is a different string
/// and the same directory.
///
/// Raw first, then canonical. Raw alone never fires for a Project herdr names through a
/// symlink — `/var` against the `/private/var` the user's shell showed them, which is every
/// Project under `/var` on macOS. Canonical alone drops every Override whose directory does
/// not resolve on this machine, which is the one state in which the connection the user
/// wrote down by hand is all there is.
fn keyed<'a>(
    projects: &'a toml::Table,
    project: &Path,
    host: &dyn Host,
) -> Option<&'a toml::Value> {
    projects
        .iter()
        .find(|(key, _)| Path::new(key.as_str()) == project)
        .or_else(|| {
            let canonical = host.canonicalize(project)?;
            projects.iter().find(|(key, _)| {
                host.canonicalize(Path::new(key.as_str())).as_ref() == Some(&canonical)
            })
        })
        .map(|(_, pinned)| pinned)
}

/// One entry of the `projects` table as an Override, or `None` if it does not carry one.
///
/// Both fields are required: a connection with no label cannot be announced, and announcing
/// which database is open is what makes the Pane safe to work in (ADR-0006). An entry missing
/// either declines for the Project it names rather than falling back to the Strategy chain:
/// the user pinned this Project, so a Candidate resolved around a broken pin would connect
/// somewhere they believe is overridden.
///
/// An empty value is one that is missing, the rule every tier here reads a value under. A
/// written-out `dsn = ""` is no connection, and `label = ""` announces nothing.
fn stated(pinned: &toml::Value) -> Option<Override> {
    Some(Override {
        dsn: written(pinned, "dsn")?.to_string(),
        label: written(pinned, "label")?.to_string(),
        read_only: read_only(pinned),
    })
}

/// The string `key` holds in this entry, or `None` where the entry states nothing under
/// `key`, an empty string counting as nothing.
fn written<'a>(pinned: &'a toml::Value, key: &str) -> Option<&'a str> {
    pinned
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|stated| !stated.is_empty())
}

/// Whether the connection opens read-only, which it does unless the entry says outright that
/// it does not.
///
/// The default is inverted from every discovered Candidate's (`docker.rs`), and the asymmetry
/// is the point: a local container is something an agent just wrote and spot-editing it is
/// what the plugin is for, whereas an Override is by construction the route to something the
/// Strategies refuse to infer, which correlates with "someone else may be using this"
/// (ADR-0005). So everything that is not the boolean `false` — a missing key, a key of the
/// wrong type, a `read_only = "false"` written as a string — falls to read-only. Falling the
/// other way would make one typo a silent write-enable on a shared database.
fn read_only(pinned: &toml::Value) -> bool {
    pinned
        .get("read_only")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true)
}

impl Override {
    /// The Pane title: the user's own label, and that this connection came from an Override
    /// rather than from anything the plugin worked out.
    ///
    /// Saying so is the point. An Override is the one connection that may be remote and the
    /// one the Strategies never vouched for, so a Pane that looked like every other Pane
    /// would be the least safe one on screen (AC 21, ADR-0006). Never the DSN it is pinned
    /// to: the title is on screen for as long as the Pane is.
    pub fn title(&self) -> String {
        format!("{} · override", self.label)
    }
}
