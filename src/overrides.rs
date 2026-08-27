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
    /// No config directory, no file, or a file naming no Override for this Project.
    Silent,
    Pinned(Override),
}

/// One connection a user pinned to a Project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub dsn: String,
    pub label: String,
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
    /// An empty value has named no directory, the same rule the invocation context and the
    /// Pane id are read under. It cannot be allowed through as a path: joining the file name
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
        let Some(raw) = host.read_file(&directory.join(FILE)) else {
            return Overridden::Silent;
        };
        let Ok(parsed) = toml::from_str::<toml::Value>(&raw) else {
            return Overridden::Silent;
        };
        let Some(projects) = parsed.get(PROJECTS).and_then(toml::Value::as_table) else {
            return Overridden::Silent;
        };
        match keyed(projects, project, host) {
            Some(pinned) => stated(pinned),
            None => Overridden::Silent,
        }
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
    if let Some((_, pinned)) = projects
        .iter()
        .find(|(key, _)| Path::new(key.as_str()) == project)
    {
        return Some(pinned);
    }
    let canonical = host.canonicalize(project)?;
    projects
        .iter()
        .find(|(key, _)| host.canonicalize(Path::new(key.as_str())).as_ref() == Some(&canonical))
        .map(|(_, pinned)| pinned)
}

/// One entry of the `projects` table as an Override, or `Silent` if it does not carry one.
///
/// Both fields are required, and an entry missing either says nothing rather than taking the
/// rest of the file down with it: a connection with no label cannot be announced, and
/// announcing which database is open is what makes the Pane safe to work in (ADR-0006).
fn stated(pinned: &toml::Value) -> Overridden {
    let addressed = pinned.get("dsn").and_then(toml::Value::as_str);
    let label = pinned.get("label").and_then(toml::Value::as_str);
    match (addressed, label) {
        (Some(addressed), Some(label)) => Overridden::Pinned(Override {
            dsn: addressed.to_string(),
            label: label.to_string(),
        }),
        _ => Overridden::Silent,
    }
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
