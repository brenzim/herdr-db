//! The Compose-renderer Strategy: the databases a Project *declares*, matched to the
//! containers that are actually running them.
//!
//! Compose's own configuration renderer is the authority on Compose's semantics, so the
//! multi-file merge, `!override` and `${VAR:-default}` are answered by the tool that defines
//! them rather than reimplemented here against a YAML parser.
//!
//! What the render is for is narrow, and ADR-0008 is the whole of it: it supplies an
//! *identity* — which service is a database, and which running container belongs to the
//! Stack — and never a port. Every part of the DSN comes off the container, exactly as the
//! Live Docker Strategy reads one, because a declared port is a statement of intent that
//! either nothing is listening on or that the container already disagrees with.

use std::path::{Path, PathBuf};

use crate::candidate::{Candidate, Origin};
use crate::docker::{self, Inspected};
use crate::host::Host;

/// The command this Strategy renders with, and the one binary it needs present. Absent, it
/// is a Strategy that finds nothing rather than a fault.
const PROGRAM: &str = "docker";

/// The render, asked for the way `docker compose up` in that directory would have resolved
/// the Stack: no `-f`, so Compose applies its own filename precedence and loads its own
/// `docker-compose.override.yml`, and every profile, because a service behind `profiles:`
/// is invisible to a default render and is still a database.
const RENDER: [&str; 6] = ["compose", "--profile", "*", "config", "--format", "json"];

/// The filenames Compose loads without being told. A directory holding any of them is one
/// Stack however many of them it holds: Compose applies its own precedence between them and
/// merges nothing, so a Stack per filename would render the same directory up to four times.
const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Directories a Project's own Stacks are never in, and that a walk of a real repository
/// would otherwise spend its whole budget inside.
const SKIPPED: [&str; 5] = ["node_modules", "vendor", "target", "dist", ".git"];

/// How far beneath the Project a Stack is looked for, counting the Project itself as 0.
/// `infra/` and `docker/` are 1 and `apps/api/` is 2, so this is every layout seen plus a
/// level of headroom.
const DEPTH: usize = 3;

/// The container port a PostgreSQL answers on, and the one a declared port must target for
/// the service publishing it to be a database.
const POSTGRES_PORT: u64 = 5432;

/// The environment keys that say a service is a PostgreSQL. Their *presence* is the signal
/// and their value is not: an unset `${PGPASSWORD}` renders as an empty string, and a Stack
/// that interpolates its credentials from the environment is exactly the kind this
/// Strategy has to identify.
const IDENTIFYING: [&str; 3] = ["POSTGRES_USER", "POSTGRES_DB", "POSTGRES_PASSWORD"];

/// The label naming which service of a Stack a container was brought up as.
const COMPOSE_SERVICE: &str = "com.docker.compose.service";

/// The label numbering a container among the replicas of its service. Compose stores it as
/// a string, so `"10"` precedes `"2"` read as text and the tenth replica would be resolved
/// as the first.
const COMPOSE_NUMBER: &str = "com.docker.compose.container-number";

/// The Candidates the Stacks beneath `project` offer, given the containers already swept.
pub fn candidates(project: &Path, host: &dyn Host, sweep: &[Inspected]) -> Vec<Candidate> {
    // Once, before anything is compared: every Stack directory is then built by descending
    // from a canonical root, and the Compose labels are canonicalised to meet them.
    let Some(project_root) = host.canonicalize(project) else {
        return Vec::new();
    };
    let mut found: Vec<Candidate> = stacks(&project_root, host)
        .iter()
        .filter_map(|stack| Some((stack, render(stack, host)?)))
        .flat_map(|(stack, rendered)| services(&rendered, stack, host, sweep))
        .collect();
    // Stacks are walked in a stable order already, but a Stack can declare several
    // databases and the rank the title states must not depend on the order a JSON object
    // happens to iterate in.
    found.sort_by(|one, other| (one.port, &one.container).cmp(&(other.port, &other.container)));
    found
}

/// Every Stack directory beneath `root`, in a stable order. `root` itself is one when it
/// holds a Compose file — the commonest layout there is.
fn stacks(root: &Path, host: &dyn Host) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(root, 0, host, &mut found);
    found.sort();
    found
}

fn walk(directory: &Path, depth: usize, host: &dyn Host, found: &mut Vec<PathBuf>) {
    let entries = host.list_dir(directory);
    if entries.iter().any(|entry| is_compose_file(entry)) {
        found.push(directory.to_path_buf());
    }
    if depth == DEPTH {
        return;
    }
    for entry in entries {
        let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if SKIPPED.contains(&name) {
            continue;
        }
        // A directory reached through a symlink resolves to somewhere other than where it
        // was reached from — which is how a walk of the Project leaves the Project, and
        // resolves a database belonging to whatever the link points at. Every path here is
        // built by descending from a canonical root, so this comparison is exact.
        if host.canonicalize(&entry).as_deref() != Some(entry.as_path()) {
            continue;
        }
        // Whether the entry is a directory is never asked: a Host lists a file as nothing,
        // so descending into one is a silent no-op rather than a question with an answer
        // that could be wrong.
        walk(&entry, depth + 1, host, found);
    }
}

fn is_compose_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| COMPOSE_FILES.contains(&name))
}

/// What Compose says the Stack in `directory` is, or `None` when it would not say.
///
/// Run in the Stack's own directory, which is what makes `.env` resolve from there and the
/// override file load itself. A render that fails — an unset `${VAR:?}`, a file that is not
/// valid YAML — exits non-zero with an empty stdout, and that is this Stack saying nothing
/// rather than the Project failing to resolve: the other Stacks are still rendered. Compose
/// writes its warnings to stderr while exiting 0, so only the status is read.
fn render(directory: &Path, host: &dyn Host) -> Option<serde_json::Value> {
    let rendered = host.run(PROGRAM, &RENDER, directory)?;
    if rendered.status != 0 {
        return None;
    }
    serde_json::from_str(&rendered.stdout).ok()
}

/// The Candidates a rendered Stack offers: one per qualifying service that something is
/// running.
fn services(
    rendered: &serde_json::Value,
    directory: &Path,
    host: &dyn Host,
    sweep: &[Inspected],
) -> Vec<Candidate> {
    let Some(services) = rendered.get("services").and_then(|it| it.as_object()) else {
        return Vec::new();
    };
    services
        .iter()
        .filter(|(_, service)| qualifies(service))
        .filter_map(|(name, _)| container(sweep, directory, name, host))
        .filter_map(|inspected| docker::connection(inspected, Origin::Compose))
        .collect()
}

/// Whether a rendered service is a PostgreSQL.
///
/// The union of three signals rather than the image alone, because a service with a
/// `build:` stanza renders with no `image` key at all — and that is the case this Strategy
/// exists for, the one the Live Docker Strategy's image filter cannot see. A false positive
/// costs nothing: the service still has to have a running container publishing a reachable
/// host port before it is a Candidate.
fn qualifies(service: &serde_json::Value) -> bool {
    let image = service
        .get("image")
        .and_then(|it| it.as_str())
        .is_some_and(|image| docker::IMAGES.iter().any(|known| image.contains(known)));
    image || targets_postgres(service) || names_postgres(service)
}

/// Whether the service declares a port whose *container* side is PostgreSQL's. The host side
/// is deliberately not read: it is the port ADR-0008 forbids putting in a DSN, and it is
/// absent altogether from a port that is exposed and never published.
fn targets_postgres(service: &serde_json::Value) -> bool {
    service
        .get("ports")
        .and_then(|it| it.as_array())
        .is_some_and(|ports| {
            ports
                .iter()
                .any(|port| port.get("target").and_then(|it| it.as_u64()) == Some(POSTGRES_PORT))
        })
}

/// Whether the service's environment names one of the variables the official image
/// configures itself from. The render resolves `env_file:` into this block, which makes it
/// look exactly like a source of credentials: it is an identification signal and nothing
/// else (ADR-0008).
fn names_postgres(service: &serde_json::Value) -> bool {
    service
        .get("environment")
        .and_then(|it| it.as_object())
        .is_some_and(|environment| {
            IDENTIFYING
                .iter()
                .any(|name| environment.contains_key(*name))
        })
}

/// The running container behind `service` of the Stack in `directory`, or `None` when
/// nothing is running it.
///
/// Attributed by the Stack's *own* directory and not by the Project containing it: two
/// Stacks in one Project may each have a service called `db`, and a container matched on
/// being somewhere underneath the Project is a Launch into the other one's database, under
/// a title stating this one's.
fn container<'a>(
    sweep: &'a [Inspected],
    directory: &Path,
    service: &str,
    host: &dyn Host,
) -> Option<&'a Inspected> {
    sweep
        .iter()
        .filter(|inspected| brought_up_by(&inspected.json, directory, service, host))
        // Scaled, the service has several containers and only one of them may be resolved:
        // whichever Docker happened to list first would differ run to run, and only
        // consistent behaviour is learnable.
        .min_by_key(|inspected| number(&inspected.json))
}

fn brought_up_by(
    inspected: &serde_json::Value,
    directory: &Path,
    service: &str,
    host: &dyn Host,
) -> bool {
    let Some(labels) = inspected.get("Config").and_then(|it| it.get("Labels")) else {
        return false;
    };
    let named = labels
        .get(COMPOSE_SERVICE)
        .and_then(|it| it.as_str())
        .is_some_and(|named| named == service);
    // Both sides canonical, as they are for the Project: macOS reports the same directory as
    // `/var/…` and as `/private/var/…`, and compared as they arrive it fails to equal itself.
    let same_directory = labels
        .get(docker::COMPOSE_WORKING_DIR)
        .and_then(|it| it.as_str())
        .and_then(|working_dir| host.canonicalize(Path::new(working_dir)))
        .is_some_and(|working_dir| working_dir == directory);
    named && same_directory
}

/// Which replica of its service a container is. Parsed, because Compose stores the number as
/// a string; a container that does not carry one sorts last rather than stopping being a
/// container.
fn number(inspected: &serde_json::Value) -> u64 {
    inspected
        .get("Config")
        .and_then(|it| it.get("Labels"))
        .and_then(|labels| labels.get(COMPOSE_NUMBER))
        .and_then(|it| it.as_str())
        .and_then(|number| number.parse().ok())
        .unwrap_or(u64::MAX)
}
