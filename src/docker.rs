//! The Live Docker Strategy: running containers, matched to the Project by the Compose
//! working directory they were brought up from.
//!
//! Ranked above every declaration-based Strategy because a bound host port is ground truth
//! — a container Docker reports as running, publishing a port, is a database that is
//! actually there, where a compose file is only a statement of intent.

use std::path::Path;

use crate::candidate::{Candidate, Origin};
use crate::host::Host;

/// The command this Strategy asks about the machine's containers, and the one binary it
/// needs present. Absent, it is a Strategy that finds nothing rather than a fault.
const PROGRAM: &str = "docker";

/// The Candidates the running containers of this machine offer `project`.
pub fn candidates(project: &Path, host: &dyn Host) -> Vec<Candidate> {
    // Once, before anything is compared: the Project arrives as herdr names it, and the
    // labels arrive as Compose recorded them, and on macOS the same directory is `/var/…`
    // to one and `/private/var/…` to the other.
    let Some(project_root) = host.canonicalize(project) else {
        return Vec::new();
    };
    let mut found: Vec<Candidate> = running(project, host)
        .iter()
        .filter_map(|id| host.run(PROGRAM, &["inspect", id], project))
        .filter(|inspected| inspected.status == 0)
        .filter_map(|inspected| serde_json::from_str::<serde_json::Value>(&inspected.stdout).ok())
        .filter_map(|inspected| candidate(inspected.get(0)?, &project_root, host))
        .collect();
    // `docker ps` lists in whichever order it lists, and the rank the title states must not
    // be one of them: only consistent behaviour is learnable, so a Project resolves to the
    // same Candidate every run. Stated as the two things the order is, rather than derived
    // from the struct, whose field order is not an opinion about which database to open.
    found.sort_by(|one, other| (one.port, &one.container).cmp(&(other.port, &other.container)));
    found
}

/// The ids of every container Docker reports as running. Liveness is asked of Docker and
/// of nothing else: a port that answers proves nothing, because Docker Desktop holds
/// `*:5432` open with no container behind it.
fn running(project: &Path, host: &dyn Host) -> Vec<String> {
    let Some(listed) = host.run(
        PROGRAM,
        &["ps", "--filter", "status=running", "--format", "{{.ID}}"],
        project,
    ) else {
        return Vec::new();
    };
    listed
        .stdout
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

/// The images that are a PostgreSQL, matched as substrings of what the container was
/// built from. The false positive this admits is `postgrest/postgrest`, which contains
/// "postgres" and is not a database — it is refused by publishing 3000 rather than 5432,
/// so tightening this filter would buy nothing and would drop distributions that name
/// themselves in ways nobody has thought of yet.
const IMAGES: [&str; 4] = ["postgres", "pgvector", "timescale", "supabase"];

/// The label Compose stamps every container it brings up with, naming the directory the
/// compose file was run from. The one thing that attributes a container to a Project: a
/// container without it cannot be attributed to any Project at all, and guessing which one
/// owns it is how the wrong database gets edited.
const COMPOSE_WORKING_DIR: &str = "com.docker.compose.project.working_dir";

/// The role the official image starts as when nothing names one.
const DEFAULT_ROLE: &str = "postgres";

/// One container's `docker inspect` object as a Candidate, or `None` if it is not one of
/// `project`'s: the wrong image, the wrong directory, or no reachable port.
fn candidate(
    inspected: &serde_json::Value,
    project_root: &Path,
    host: &dyn Host,
) -> Option<Candidate> {
    let config = inspected.get("Config")?;
    let image = config.get("Image")?.as_str()?;
    if !IMAGES.iter().any(|known| image.contains(known)) {
        return None;
    }
    let working_dir = config.get("Labels")?.get(COMPOSE_WORKING_DIR)?.as_str()?;
    // A label naming a directory this machine does not have resolves to nothing, and a
    // container that cannot be placed is not this Project's: falling back to the raw path
    // would match it on its spelling, which is what canonicalising is here to stop.
    let working_dir = host.canonicalize(Path::new(working_dir))?;
    // Component-wise, which is what `Path::starts_with` compares and what comparing the
    // display forms as text does not: `/foo/bar-baz` is not under `/foo/bar`, and it is a
    // different repository with a different database.
    if !working_dir.starts_with(project_root) {
        return None;
    }
    // The image's own defaults, because they are what the container that is running did:
    // an unset `POSTGRES_USER` started it as `postgres`, and an unset `POSTGRES_DB` gave it
    // a database named after whichever role that resolved to. An unset `POSTGRES_PASSWORD`
    // stays unset — a guessed password turns a connection that would have worked into one
    // that is refused.
    let role = setting(config, "POSTGRES_USER").unwrap_or(DEFAULT_ROLE);
    Some(Candidate {
        origin: Origin::Docker,
        database: setting(config, "POSTGRES_DB").unwrap_or(role).to_string(),
        role: role.to_string(),
        password: setting(config, "POSTGRES_PASSWORD").map(str::to_string),
        port: published_port(inspected)?,
        container: name(inspected),
        read_only: false,
    })
}

/// What the container's environment says `name` is, or `None` when it does not say. The
/// official image reads its own configuration from here, and so does everything built on
/// it, which is why this is the environment of the container that is running rather than
/// of the compose file that may or may not have started it.
///
/// An empty value has said nothing: Compose writes one out of an unset shell variable, and
/// the image reads it as absent — the same rule the invocation context's tiers follow.
///
/// Split on the *first* `=` and no other: base64 pads with `=`, so a generated password
/// routinely carries one, and a value cut short is a password that is quietly wrong.
fn setting<'a>(config: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    config
        .get("Env")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|entry| entry.split_once('='))
        .find(|(key, value)| *key == name && !value.is_empty())
        .map(|(_, value)| value)
}

/// The container's name as the user reads it in `docker ps`: Docker reports it with a
/// leading `/`, and the tie-break between two Candidates on one port sorts on it. Empty
/// only in an answer Docker has never given — a container that names itself nothing ties
/// on nothing rather than stopping being a Candidate.
fn name(inspected: &serde_json::Value) -> String {
    inspected
        .get("Name")
        .and_then(serde_json::Value::as_str)
        .map(|name| name.strip_prefix('/').unwrap_or(name))
        .unwrap_or_default()
        .to_string()
}

/// The host port the container's PostgreSQL is reachable on, or `None` when it is not:
/// Docker reports a port the image exposes whether or not anything bound it on the host,
/// and only a binding can be put in a DSN. Keyed on `5432/tcp` exactly, because a stack
/// that also publishes an exporter publishes two ports and only one of them is a database.
///
/// One port per container, always: Docker repeats a binding once per host address it bound
/// it on, and a database reachable on both IPv4 and IPv6 is one database. The lowest of the
/// ones that can be reached, since only a reachable binding is one the DSN can name.
fn published_port(inspected: &serde_json::Value) -> Option<u16> {
    inspected
        .get("NetworkSettings")?
        .get("Ports")?
        .get("5432/tcp")?
        .as_array()?
        .iter()
        .filter(|binding| reachable(binding))
        .filter_map(|binding| binding.get("HostPort")?.as_str()?.parse().ok())
        .min()
}

/// The host addresses a binding made on which the IPv4 loopback the DSN names can reach: the
/// two wildcards Docker writes for an unrestricted publish, that loopback itself, and the
/// empty address its API writes when it names none.
///
/// `::1` is deliberately not one of them: a binding made there answers on the IPv6 loopback
/// alone, and the DSN's `127.0.0.1` is refused by it.
const LOOPBACK: [&str; 4] = ["", "0.0.0.0", "127.0.0.1", "::"];

/// Whether the host can reach `binding` at the address a Candidate's DSN is written with.
/// A binding carries the interface it was made on, and `ports: ["192.168.1.10:5434:5432"]`
/// is bound on that one alone: put in a DSN addressed at the loopback it is a connection
/// refused, under a Pane title stating which database was resolved.
fn reachable(binding: &serde_json::Value) -> bool {
    binding
        .get("HostIp")
        .map_or(Some(""), serde_json::Value::as_str)
        .is_some_and(|address| LOOPBACK.contains(&address))
}
