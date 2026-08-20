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
use std::time::{Duration, Instant};

use crate::candidate::{Candidate, Origin};
use crate::docker::{self, Inspected, PROGRAM};
use crate::host::Host;

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

/// How long a Project may go on starting renders for.
///
/// Spent *between* renders and never during one: the check is made before each, so a Stack
/// that starts just inside the budget still runs to its own `host::DEADLINE` and the worst
/// case is the budget plus one per-command deadline.
///
/// Each Stack is a `docker compose config` that may take `host::DEADLINE` on its own, and
/// nothing bounds how many Stacks a Project holds: the Project is not always a repository
/// root, and with no Worktree and no focused Pane it is herdr's workspace — the directory
/// every checkout on the machine lives under. Against a half-dead Docker, the state the
/// per-command deadline exists for, ten Stacks with no shared budget is a Pane drawing
/// nothing for a minute and a half. Generous against a healthy render, which is a fraction
/// of a second, so a real monorepo still renders every Stack it has.
const BUDGET: Duration = Duration::from_secs(20);

/// The label naming which service of a Stack a container was brought up as.
const COMPOSE_SERVICE: &str = "com.docker.compose.service";

/// The label numbering a container among the replicas of its service. Compose stores it as
/// a string, so `"10"` precedes `"2"` read as text and the tenth replica would be resolved
/// as the first.
const COMPOSE_NUMBER: &str = "com.docker.compose.container-number";

/// What the Stacks beneath a Project say, taken together: the connections their running
/// containers offer, and the databases they declare that nothing is running.
#[derive(Debug, Default)]
pub struct Rendered {
    pub candidates: Vec<Candidate>,
    /// Every one the budget allowed to be rendered, ordered by Stack directory. Told about
    /// one of two, the user starts that one, retries, and reads the same screen back naming
    /// the other.
    pub stopped: Vec<Stopped>,
}

/// One database a Stack declares with no container behind it — a statement of intent that
/// nothing has acted on, which is the commonest state a machine is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    /// The container Compose would bring the service up as, `<name>-<service>-1`. `<name>`
    /// is the render's *own* resolved project name, which has already decided between
    /// `COMPOSE_PROJECT_NAME`, a top-level `name:` and the directory default — deriving it
    /// from the directory here would name a container that does not exist for two of those
    /// three.
    pub container: String,
    /// The service the Stack declares it as, which is what the remedy has to name.
    pub service: String,
    /// The Stack's Compose file as Compose's own precedence picked it out of the directory,
    /// relative to the Project: the absolute prefix is the part the user already knows.
    pub file: PathBuf,
    /// The Stack's own directory, absolute, because the remedy is run from there and a
    /// Pane's working directory is this plugin's install directory (ADR-0004).
    pub directory: PathBuf,
}

/// One Stack: a directory, absolute, and the single file Compose would load in it, held
/// relative to the Project because displaying it is the only thing it is for.
struct Stack {
    directory: PathBuf,
    file: PathBuf,
}

/// What the Stacks beneath `project` offer, given the containers already swept.
///
/// The Candidates arrive in whatever order the Stacks and their services were read in:
/// ordering them is the chain's business and not a Strategy's, and `plan::ranked` sorts
/// every Strategy's together on a key this one's own sort could only be a prefix of.
pub fn candidates(project: &Path, host: &dyn Host, sweep: &[Inspected]) -> Rendered {
    candidates_within(project, host, sweep, BUDGET)
}

/// [`candidates`] under the budget given rather than `BUDGET`. Exists so that a test can
/// prove the budget ends the loop in milliseconds instead of paying it in wall clock; the
/// Pane always goes through `candidates`.
pub fn candidates_within(
    project: &Path,
    host: &dyn Host,
    sweep: &[Inspected],
    budget: Duration,
) -> Rendered {
    // Once, before anything is compared: every Stack directory is then built by descending
    // from a canonical root, and the Compose labels are canonicalised to meet them.
    let Some(project_root) = host.canonicalize(project) else {
        return Rendered::default();
    };
    let expires = Instant::now() + budget;
    let mut found = Rendered::default();
    for stack in stacks(&project_root, host) {
        // A Stack the budget stops from being rendered says nothing, exactly as one that
        // refused to render says nothing: whatever the Stacks already read said still
        // stands, and the rest of the chain still runs.
        if Instant::now() >= expires {
            break;
        }
        let Some(said) = render(&stack.directory, host) else {
            continue;
        };
        services(&said, &stack, host, sweep, &mut found);
    }
    // Nothing downstream orders these, so the order the Decline reads in is stated here
    // rather than inherited: a Stack can declare several databases, and neither the walk's
    // order nor the render's own map type is this list's to depend on.
    found.stopped.sort_by(|one, other| {
        (&one.directory, &one.service).cmp(&(&other.directory, &other.service))
    });
    found
}

/// Every Stack beneath `root`, in a stable order. `root` itself is one when it holds a
/// Compose file — the commonest layout there is.
fn stacks(root: &Path, host: &dyn Host) -> Vec<Stack> {
    let mut found = Vec::new();
    walk(root, 0, host, &mut found);
    for stack in &mut found {
        // Once and here, rather than per declared database.
        stack.file = stack
            .file
            .strip_prefix(root)
            .unwrap_or(&stack.file)
            .to_path_buf();
    }
    found.sort_by(|one, other| one.directory.cmp(&other.directory));
    found
}

fn walk(directory: &Path, depth: usize, host: &dyn Host, found: &mut Vec<Stack>) {
    let entries = host.list_dir(directory);
    if let Some(file) = compose_file(&entries) {
        found.push(Stack {
            directory: directory.to_path_buf(),
            file,
        });
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

/// The one file of `entries` Compose would load, or `None` when the directory is not a
/// Stack. `COMPOSE_FILES` is in Compose's own precedence order, so the first name that is
/// present is the one that wins — and the other three are named by nothing, because naming
/// them would send the user to edit a file that declared nothing.
fn compose_file(entries: &[PathBuf]) -> Option<PathBuf> {
    COMPOSE_FILES.iter().find_map(|wanted| {
        entries
            .iter()
            .find(|entry| entry.file_name().and_then(|name| name.to_str()) == Some(*wanted))
            .cloned()
    })
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

/// What a rendered Stack offers, appended to `found`: a Candidate for every qualifying
/// service something is running, and a `Stopped` for every service that runs a PostgreSQL
/// itself and has nothing running it.
///
/// The two are exclusive and neither is a fallback for the other. A service whose container
/// is running but publishes nothing reachable is neither: it is not a connection, and
/// `docker compose up -d` would not change that, so telling the user to run it would send
/// them round a loop.
fn services(
    said: &serde_json::Value,
    stack: &Stack,
    host: &dyn Host,
    sweep: &[Inspected],
    found: &mut Rendered,
) {
    let Some(services) = said.get("services").and_then(|it| it.as_object()) else {
        return;
    };
    // Once per Stack: it is the render's answer about the Stack, not about any one service.
    let project = project_name(said, &stack.directory);
    for (service, declared) in services {
        if !qualifies(declared) {
            continue;
        }
        match container(sweep, &stack.directory, service, host) {
            Some(inspected) => found
                .candidates
                .extend(docker::connection(inspected, Origin::Compose)),
            None if runs_postgres(declared) => found.stopped.push(Stopped {
                container: format!("{project}-{service}-1"),
                service: service.clone(),
                file: stack.file.clone(),
                directory: stack.directory.clone(),
            }),
            None => {}
        }
    }
}

/// The project name Compose would prefix the Stack's container names with, which is what
/// makes one `<name>-<service>-1`.
///
/// Read back out of the render rather than derived, because by the time Compose writes it
/// there it has already resolved `COMPOSE_PROJECT_NAME` over a top-level `name:` over the
/// directory the Stack sits in. The fallback is that same directory — Compose's own
/// default, and reachable only from a render that named no project at all.
fn project_name<'a>(said: &'a serde_json::Value, directory: &'a Path) -> &'a str {
    said.get("name")
        .and_then(|it| it.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
        })
}

/// Whether a rendered service is a PostgreSQL.
///
/// The union of three signals rather than the image alone, because a service with a
/// `build:` stanza renders with no `image` key at all — and that is the case this Strategy
/// exists for, the one the Live Docker Strategy's image filter cannot see. A false positive
/// costs nothing on the Candidate path: the service still has to have a running container
/// publishing a reachable host port before it is a Candidate. It is not free on the other
/// branch of `services`, where a service with no container behind it is named to the user as
/// a declared database — which is why the Decline asks the stricter `runs_postgres` instead.
fn qualifies(service: &serde_json::Value) -> bool {
    names_image(service) || targets_postgres(service) || names_postgres(service)
}

/// Whether a qualifying service is the PostgreSQL itself, rather than something the Stack
/// hands its credentials to. The stricter question, and the one the Decline has to ask.
///
/// `POSTGRES_USER` and its two companions are as much the mark of the app that connects to
/// the database as of the database: the ordinary layout puts all three in the `api` service
/// as well. On the Candidate path that costs nothing, because the app publishes no
/// `5432/tcp` and yields no Candidate. Here it would name `<project>-api-1` as a declared
/// database and offer a remedy that starts the app, changes nothing, and brings back the
/// same screen on retry — so the environment alone is not enough, and either the image or a
/// port targeting 5432 has to say so.
fn runs_postgres(service: &serde_json::Value) -> bool {
    names_image(service) || targets_postgres(service)
}

/// Whether the service declares an image that is a PostgreSQL, matched the way the Live
/// Docker Strategy matches a running container's — the same list, so a Stack and the
/// container it brought up cannot disagree about what a PostgreSQL image is.
fn names_image(service: &serde_json::Value) -> bool {
    service
        .get("image")
        .and_then(|it| it.as_str())
        .is_some_and(|image| docker::IMAGES.iter().any(|known| image.contains(known)))
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
    let Some(labels) = labels(inspected) else {
        return false;
    };
    // The service first, and the directory only if it matched: reading the label is free
    // where resolving the directory is a `realpath` of every one of its components, asked
    // once per container for every service of every Stack.
    if labels.get(COMPOSE_SERVICE).and_then(|it| it.as_str()) != Some(service) {
        return false;
    }
    // Both sides canonical, as they are for the Project: macOS reports the same directory as
    // `/var/…` and as `/private/var/…`, and compared as they arrive it fails to equal itself.
    labels
        .get(docker::COMPOSE_WORKING_DIR)
        .and_then(|it| it.as_str())
        .and_then(|working_dir| host.canonicalize(Path::new(working_dir)))
        .is_some_and(|working_dir| working_dir == directory)
}

/// The label map a container carries, or `None` when it carries none — which is the shape
/// `docker inspect` reports and not one this crate chose.
fn labels(inspected: &serde_json::Value) -> Option<&serde_json::Value> {
    inspected.get("Config").and_then(|it| it.get("Labels"))
}

/// Which replica of its service a container is. Parsed, because Compose stores the number as
/// a string; a container that does not carry one sorts last rather than stopping being a
/// container.
fn number(inspected: &serde_json::Value) -> u64 {
    labels(inspected)
        .and_then(|labels| labels.get(COMPOSE_NUMBER))
        .and_then(|it| it.as_str())
        .and_then(|number| number.parse().ok())
        .unwrap_or(u64::MAX)
}
