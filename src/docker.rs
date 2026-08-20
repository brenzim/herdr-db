//! The Live Docker Strategy: running containers, matched to the Project by the Compose
//! working directory they were brought up from.
//!
//! The Strategy that ranks above every declaration-based one: a bound host port is ground
//! truth — a container Docker reports as running, publishing a port, is a database that is
//! actually there, where a compose file is only a statement of intent.
//!
//! What a sweep is and what a container offers are read from here by the Compose Strategy
//! too, so that two Strategies vouching for one container cannot disagree about it
//! (ADR-0008). Matching a container to a Project is this Strategy's alone.

use std::path::Path;

use crate::candidate::{Candidate, Origin};
use crate::host::Host;

/// The one binary either Strategy needs present — this one asks it about the machine's
/// containers, and the Compose Strategy renders with it. Absent, it is a Strategy that
/// finds nothing rather than a fault. Spelled once, so the two can never drift apart about
/// which binary they shell out to.
pub(crate) const PROGRAM: &str = "docker";

/// One running container: the id `docker ps` listed it under, and the object
/// `docker inspect` answered with.
pub struct Inspected {
    pub id: String,
    pub json: serde_json::Value,
}

/// Every container Docker reports as running, inspected. Swept once per Plan and read by
/// every Strategy that needs to know what is alive: two sweeps could disagree about the
/// same machine, and a Candidate deduplicated against a container the other Strategy never
/// saw is a Candidate deduplicated against nothing.
pub fn sweep(project: &Path, host: &dyn Host) -> Vec<Inspected> {
    running(project, host)
        .iter()
        .filter_map(|id| {
            let inspected = host.run(PROGRAM, &["inspect", id], project)?;
            if inspected.status != 0 {
                return None;
            }
            // `docker inspect` answers with an array of one; unwrapped here so that every
            // reader of a sweep sees a container rather than a list holding one. Moved out
            // of the array rather than cloned out of it: an inspect object is a tree of
            // tens of kilobytes, and copying all of it to drop a one-element wrapper is
            // hundreds of allocations per container that are garbage immediately.
            let serde_json::Value::Array(said) = serde_json::from_str(&inspected.stdout).ok()?
            else {
                return None;
            };
            Some(Inspected {
                id: id.clone(),
                json: said.into_iter().next()?,
            })
        })
        .collect()
}

/// The Candidates the running containers of this machine offer `project`, in whatever order
/// `docker ps` listed them. Ordering them is the chain's business and not a Strategy's:
/// `plan::ranked` sorts every Strategy's Candidates together, on a key this one's own sort
/// could only be a prefix of.
pub fn candidates(project: &Path, host: &dyn Host, sweep: &[Inspected]) -> Vec<Candidate> {
    // Once, before anything is compared: the Project arrives as herdr names it, and the
    // labels arrive as Compose recorded them, and on macOS the same directory is `/var/…`
    // to one and `/private/var/…` to the other.
    let Some(project_root) = host.canonicalize(project) else {
        return Vec::new();
    };
    sweep
        .iter()
        .filter_map(|inspected| candidate(inspected, &project_root, host))
        .collect()
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

/// The images that are a PostgreSQL, matched as substrings of `Config.Image` — the image
/// reference the container was created from, which for a Compose service with a `build:`
/// stanza is the locally built `<project>-<service>` tag. The false positive this admits is
/// `postgrest/postgrest`, which contains
/// "postgres" and is not a database — it is refused by publishing 3000 rather than 5432,
/// so tightening this filter would buy nothing and would drop distributions that name
/// themselves in ways nobody has thought of yet.
pub(crate) const IMAGES: [&str; 4] = ["postgres", "pgvector", "timescale", "supabase"];

/// The label Compose stamps every container it brings up with, naming the directory the
/// compose file was run from. The one thing that attributes a container to a Project: a
/// container without it cannot be attributed to any Project at all, and guessing which one
/// owns it is how the wrong database gets edited.
pub(crate) const COMPOSE_WORKING_DIR: &str = "com.docker.compose.project.working_dir";

/// The role the official image starts as when nothing names one.
const DEFAULT_ROLE: &str = "postgres";

/// One container's `docker inspect` object as a Candidate, or `None` if it is not one of
/// `project`'s: the wrong image, the wrong directory, or no reachable port.
fn candidate(inspected: &Inspected, project_root: &Path, host: &dyn Host) -> Option<Candidate> {
    let config = inspected.json.get("Config")?;
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
    connection(inspected, Origin::Docker)
}

/// One running container as the connection it offers, whichever Strategy vouched for it, or
/// `None` when nothing on the host can reach its PostgreSQL.
///
/// Shared rather than written twice: both Strategies can vouch for the same container, and
/// two readings of one container that disagreed about its port or its role would produce two
/// Candidates that no deduplication could then reconcile (ADR-0008).
pub(crate) fn connection(inspected: &Inspected, origin: Origin) -> Option<Candidate> {
    let config = inspected.json.get("Config")?;
    // The image's own defaults, because they are what the container that is running did:
    // an unset `POSTGRES_USER` started it as `postgres`, and an unset `POSTGRES_DB` gave it
    // a database named after whichever role that resolved to. An unset `POSTGRES_PASSWORD`
    // stays unset — a guessed password turns a connection that would have worked into one
    // that is refused.
    let role = setting(config, "POSTGRES_USER").unwrap_or(DEFAULT_ROLE);
    Some(Candidate {
        origin,
        // Carried forward from the `ps` listing rather than read back out of the inspect
        // object: it is the same string for both Strategies because they read one sweep,
        // where `Id` is Docker's own to report in the short form or the long one.
        id: inspected.id.clone(),
        database: setting(config, "POSTGRES_DB").unwrap_or(role).to_string(),
        role: role.to_string(),
        password: setting(config, "POSTGRES_PASSWORD").map(str::to_string),
        port: published_port(&inspected.json)?,
        container: name(&inspected.json),
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
/// One port per container, always: the lowest host port bound to the container's 5432 that
/// the DSN can reach. Docker repeats a binding once per host address it bound it on, and a
/// database reachable on both IPv4 and IPv6 is one database — but a container that genuinely
/// publishes 5432 on two different reachable host ports is resolved to the lower of them
/// with nothing said about the other.
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
