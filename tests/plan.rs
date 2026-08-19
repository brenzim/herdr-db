//! The plan seam: the one boundary the whole plugin is tested at.
//!
//! Every test here drives `plan()` and asserts on the returned `Plan`, never on which
//! Resolution Strategy ran — a Strategy is free to be rewritten, and these tests must not
//! notice. What a `Plan` says is the behaviour; how it was arrived at is not.

mod common;

use std::path::{Path, PathBuf};

use common::sources;
use herdr_db::context::InvocationContext;
use herdr_db::host::{Host, Output};
use herdr_db::plan::{Diagnosis, Launch, Plan, plan};
use serde_json::{Value, json};

/// A Host that knows nothing: no file readable, no directory populated, no command
/// runnable. Every test in this file uses it, because identifying the Project is decided
/// from the invocation context alone — a Host that answered would prove nothing.
struct SilentHost;

impl Host for SilentHost {
    fn read_file(&self, _path: &Path) -> Option<String> {
        None
    }

    fn list_dir(&self, _path: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    fn run(&self, _program: &str, _args: &[&str], _cwd: &Path) -> Option<Output> {
        None
    }

    fn canonicalize(&self, _path: &Path) -> Option<PathBuf> {
        None
    }
}

fn planned(raw: Option<&str>) -> Plan {
    plan(&InvocationContext::from_json(raw), &SilentHost)
}

#[test]
fn an_absent_invocation_context_declines_as_missing() {
    // herdr did not set HERDR_PLUGIN_CONTEXT_JSON at all. Nothing can be resolved, and the
    // user has to be told which of the three faults this is (ADR-0004).
    assert_eq!(planned(None), Plan::Decline(Diagnosis::ContextMissing));
}

#[test]
fn an_empty_invocation_context_declines_as_empty() {
    // HERDR_PLUGIN_CONTEXT_JSON is set to the empty string. A JSON parser rejects this
    // exactly as it rejects garbage, but the two are different faults with different
    // remedies, so the empty case has to be recognised before any parsing happens.
    assert_eq!(planned(Some("")), Plan::Decline(Diagnosis::ContextEmpty));
}

#[test]
fn a_malformed_invocation_context_declines_as_unreadable() {
    // Something is there but it is not JSON. This is a broken herdr, or a broken hand-set
    // variable — a different thing to say than "you set it to nothing", and the reason the
    // empty case above cannot simply fall through to the parser.
    assert_eq!(
        planned(Some("{ this is not json")),
        Plan::Decline(Diagnosis::ContextUnreadable),
    );
}

#[test]
fn a_well_formed_context_that_names_no_path_declines_as_no_project_identified() {
    // herdr spoke correctly; it simply had nothing to say — no Worktree, and neither cwd.
    // Reporting this as an unreadable context tells the user their herdr is broken when it
    // is not, and sends them looking for a fault that does not exist.
    assert_eq!(
        planned(Some(
            r#"{"worktree":null,"focused_pane_cwd":"","workspace_cwd":""}"#
        )),
        Plan::Decline(Diagnosis::NoProjectIdentified),
    );
}

/// A context of the shape herdr really sends, taken from its own embedded schema: a
/// Worktree naming the repository root, a focused Pane deeper inside it, and a workspace
/// above it. All three are path-bearing, and only one of them is the Project.
const ORDINARY: &str = r#"{"worktree":{"repo_key":"k","repo_name":"herdr-db",
    "repo_root":"/Users/b/AI/herdr-db","checkout_path":"/Users/b/AI/herdr-db",
    "is_linked_worktree":false},
    "focused_pane_cwd":"/Users/b/AI/herdr-db/src","workspace_cwd":"/Users/b/AI"}"#;

#[test]
fn identifies_the_project_from_the_worktrees_repository_root() {
    // Resolution is keyed on the Project, so every Worktree of a repository resolves alike
    // (ADR-0003) — which is only true if the root wins over the deeper Pane cwd sitting
    // inside it. The Host here answers nothing, so the Decline is what names the Project.
    assert_eq!(
        planned(Some(ORDINARY)),
        Plan::Decline(Diagnosis::NoConnectionFound {
            project: PathBuf::from("/Users/b/AI/herdr-db"),
        }),
    );
}

/// The Project, when a context identifies one. Every chain test asks this, because what
/// the chain decides is only observable as the Project a `Decline` names.
fn project_of(raw: &str) -> PathBuf {
    match planned(Some(raw)) {
        Plan::Decline(Diagnosis::NoConnectionFound { project }) => project,
        other => panic!("{raw}\nidentified no Project; it planned {other:?}"),
    }
}

#[test]
fn falls_back_to_the_focused_pane_cwd_when_there_is_no_worktree() {
    // herdr invoked from a Pane that is not in a Worktree it tracks. The Pane's own cwd is
    // the best statement about where the user is that remains (ADR-0004).
    assert_eq!(
        project_of(r#"{"worktree":null,"focused_pane_cwd":"/work/p","workspace_cwd":"/work"}"#),
        PathBuf::from("/work/p"),
    );
}

#[test]
fn falls_back_to_the_workspace_cwd_when_no_pane_is_focused() {
    // The last tier: no Worktree and no focused Pane, but herdr still knows where the
    // workspace itself lives.
    assert_eq!(
        project_of(r#"{"worktree":null,"workspace_cwd":"/work"}"#),
        PathBuf::from("/work"),
    );
}

#[test]
fn treats_an_empty_value_as_absent_at_the_tier_it_appears_at() {
    // A herdr that names a Worktree but gives it an empty root has said nothing at that
    // tier, so the chain must go on to the next one. Filtering empties once, after the
    // chain has already settled on the empty root, identifies the Project as the empty
    // path — relative, re-rooting every subsequent join at wherever the process happens to
    // be, and looking entirely successful while doing it.
    assert_eq!(
        project_of(r#"{"worktree":{"repo_root":""},"focused_pane_cwd":"/work/p"}"#),
        PathBuf::from("/work/p"),
    );
    // The same at the middle tier: an empty focused Pane cwd falls through to the workspace.
    assert_eq!(
        project_of(r#"{"worktree":null,"focused_pane_cwd":"","workspace_cwd":"/work"}"#),
        PathBuf::from("/work"),
    );
}

#[test]
fn a_tier_of_an_unexpected_type_falls_through_to_the_one_below_it() {
    // herdr's schema can grow: a property this plugin reads as a path could arrive as an
    // object or a tag. That tier has said nothing this plugin understands, which is the
    // same as saying nothing — the tiers below it are still good and must still be used.
    // Refusing the whole context instead reports a context herdr sent correctly as invalid
    // JSON, and throws away a workspace cwd that would have identified the Project.
    assert_eq!(
        project_of(r#"{"focused_pane_cwd":{"path":"/work/p"},"workspace_cwd":"/work"}"#),
        PathBuf::from("/work"),
    );
    assert_eq!(
        project_of(r#"{"worktree":"a tag herdr grew","focused_pane_cwd":"/work/p"}"#),
        PathBuf::from("/work/p"),
    );
}

#[test]
fn never_panics_whatever_the_context_turns_out_to_be() {
    // A panic in a Pane is a crash the user watches happen (ADR-0004), so there is no
    // payload — from a herdr that changed, or from a variable set by hand — that may be
    // anything other than a Decline. Nothing here asserts *which* Decline: that a payload
    // this strange is unreadable rather than pathless is not a distinction worth pinning.
    for payload in [
        "null",
        "[]",
        "3",
        r#""a bare string""#,
        r#"{"worktree":"not an object"}"#,
        r#"{"focused_pane_cwd":42}"#,
        r#"{"worktree":{"repo_root":null}}"#,
        r#"{"worktree":{}}"#,
        r#"{"a_property_this_plugin_has_never_heard_of":true}"#,
        "   ",
        "\u{0}",
    ] {
        assert!(
            matches!(planned(Some(payload)), Plan::Decline(_)),
            "the payload {payload} did not decline",
        );
    }
}

/// One container as Docker reports it: the id `docker ps` lists, and the fields of
/// `docker inspect` a Candidate is derived from. Every matching test states it by varying
/// one thing of [`container`].
struct Container {
    id: &'static str,
    name: &'static str,
    image: &'static str,
    /// The Compose label attributing the container to a directory, when it carries one.
    working_dir: Option<&'static str>,
    /// `NetworkSettings.Ports`, in Docker's own shape rather than a tidied one.
    ports: Value,
    env: Vec<&'static str>,
}

/// The ordinary case: an official PostgreSQL published on 5434, brought up by a compose
/// file in the Project's `infra/`.
fn container() -> Container {
    Container {
        id: "c0",
        name: "/orders-db",
        image: "postgres:16",
        working_dir: Some("/Users/b/AI/orders/infra"),
        ports: published(&["5434"]),
        env: vec!["POSTGRES_USER=app", "POSTGRES_DB=orders", "PATH=/usr/bin"],
    }
}

/// A `NetworkSettings.Ports` publishing the container's 5432 on each of `host_ports` —
/// Docker repeats the binding once per host address it bound it on.
fn published(host_ports: &[&str]) -> Value {
    let bindings: Vec<Value> = host_ports
        .iter()
        .map(|port| json!({"HostIp": "0.0.0.0", "HostPort": port}))
        .collect();
    json!({ "5432/tcp": bindings })
}

impl Container {
    /// What `docker inspect <id>` says: a JSON array of one object. The label map always
    /// carries something, so that a container with no Compose label is a container with
    /// *other* labels — which is what a hand-run `docker run postgres` looks like.
    fn inspected(&self) -> String {
        let mut labels = json!({"org.opencontainers.image.ref.name": "postgres"});
        if let Some(working_dir) = self.working_dir {
            labels["com.docker.compose.project.working_dir"] = json!(working_dir);
        }
        json!([{
            "Name": self.name,
            "Config": {"Image": self.image, "Labels": labels, "Env": self.env},
            "NetworkSettings": {"Ports": self.ports},
        }])
        .to_string()
    }
}

/// What the `docker` command does when a Strategy runs it.
enum Docker {
    /// Not installed: the command cannot be run at all.
    Absent,
    /// Installed with its daemon down: it runs, fails, and complains on stderr.
    DaemonDown,
    /// A working daemon, with these containers running.
    Running(Vec<Container>),
    /// A `docker` that answers every call with this, for the payloads no `Container`
    /// could produce — a herdr-side double is not the only thing that can change shape.
    Saying(String),
}

/// A Host that answers `docker` and nothing else. Paths canonicalise to themselves unless
/// `links` says otherwise, because most of them have no symlink in them and a test that
/// had to spell out every path's canonical form would say less about the one that does.
struct DockerHost {
    docker: Docker,
    /// A path prefix, and what it resolves to — or `None` for a prefix that resolves to
    /// nothing at all, which is a directory this machine does not have.
    links: Vec<(&'static str, Option<&'static str>)>,
}

impl DockerHost {
    fn running(containers: Vec<Container>) -> Self {
        Self {
            docker: Docker::Running(containers),
            links: Vec::new(),
        }
    }

    fn answering(docker: Docker) -> Self {
        Self {
            docker,
            links: Vec::new(),
        }
    }

    /// Resolves every path under `from` to the same path under `to`, or to nothing when
    /// `to` is `None`.
    fn linking(mut self, from: &'static str, to: Option<&'static str>) -> Self {
        self.links.push((from, to));
        self
    }
}

impl Host for DockerHost {
    fn read_file(&self, _path: &Path) -> Option<String> {
        None
    }

    fn list_dir(&self, _path: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    fn run(&self, program: &str, args: &[&str], _cwd: &Path) -> Option<Output> {
        if program != "docker" {
            return None;
        }
        let containers = match &self.docker {
            Docker::Absent => return None,
            Docker::DaemonDown => {
                return Some(Output {
                    status: 1,
                    stdout: String::new(),
                    stderr: "Cannot connect to the Docker daemon at \
                             unix:///var/run/docker.sock. Is the docker daemon running?"
                        .to_string(),
                });
            }
            Docker::Running(containers) => containers,
            Docker::Saying(said) => {
                return Some(Output {
                    status: 0,
                    stdout: said.clone(),
                    stderr: String::new(),
                });
            }
        };
        let said = match args {
            ["ps", "--filter", "status=running", "--format", "{{.ID}}"] => containers
                .iter()
                .map(|container| format!("{}\n", container.id))
                .collect(),
            ["inspect", id] => containers.iter().find(|it| it.id == *id)?.inspected(),
            // Any other call is one this double was never told to expect, and answering it
            // with something plausible would hide the Strategy having changed its mind
            // about how it asks Docker.
            _ => return None,
        };
        Some(Output {
            status: 0,
            stdout: said,
            stderr: String::new(),
        })
    }

    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        let raw = path.to_str()?;
        let Some((from, to)) = self
            .links
            .iter()
            .find(|(from, _)| raw == *from || raw.starts_with(&format!("{from}/")))
        else {
            return Some(path.to_path_buf());
        };
        to.map(|to| PathBuf::from(raw.replacen(from, to, 1)))
    }
}

/// The Project every Docker test resolves for.
const PROJECT: &str = "/Users/b/AI/orders";

/// Drives `plan()` for a context identifying `project`, against a scripted Docker.
fn planned_for(project: &str, host: &dyn Host) -> Plan {
    let raw = format!(r#"{{"worktree":{{"repo_root":"{project}"}}}}"#);
    plan(&InvocationContext::from_json(Some(&raw)), host)
}

fn planned_against(host: &dyn Host) -> Plan {
    planned_for(PROJECT, host)
}

/// What a Project with nothing behind it plans. Every "not a Candidate" case must produce
/// exactly this: a container that does not match is indistinguishable from no container at
/// all, and neither is a fault worth a Diagnosis of its own (AC 8).
fn found_nothing() -> Plan {
    Plan::Decline(Diagnosis::NoConnectionFound {
        project: PathBuf::from(PROJECT),
    })
}

#[test]
fn a_running_postgres_under_the_project_is_a_candidate() {
    // The whole point of the ticket: a compose file in the Project's `infra/` brings up a
    // PostgreSQL that publishes a host port, and pressing the key lands in it. What the
    // Launch *says* is asserted where the DSN and the title are built; that it launches at
    // all is the matching.
    assert!(
        matches!(
            planned_against(&DockerHost::running(vec![container()])),
            Plan::Launch(_),
        ),
        "a running PostgreSQL brought up from inside the Project was not matched to it",
    );
}

#[test]
fn a_port_that_is_exposed_but_not_published_is_not_a_candidate() {
    // Docker reports a port the image `EXPOSE`s whether or not anything on the host can
    // reach it. Without a binding there is no host port to put in a DSN, so this is not a
    // Candidate that fails to connect — it is not a Candidate (#7 owns telling the user).
    for ports in [json!({"5432/tcp": null}), json!({"5432/tcp": []})] {
        let host = DockerHost::running(vec![Container {
            ports: ports.clone(),
            ..container()
        }]);
        assert_eq!(
            planned_against(&host),
            found_nothing(),
            "the unpublished {ports} was treated as reachable from the host",
        );
    }
}

#[test]
fn only_a_binding_of_the_containers_own_5432_counts() {
    // A stack commonly runs an exporter alongside the database, and a container publishing
    // one port is not thereby publishing PostgreSQL. Keying on `5432/tcp` exactly is also
    // what makes the image filter safe: `postgrest/postgrest` contains "postgres" and
    // publishes 3000, so it never gets this far.
    let exporter = json!({"9187/tcp": [{"HostIp": "0.0.0.0", "HostPort": "9187"}]});
    assert_eq!(
        planned_against(&DockerHost::running(vec![Container {
            ports: exporter,
            ..container()
        }])),
        found_nothing(),
        "a container publishing only an exporter's port was matched as a database",
    );

    let both = json!({
        "9187/tcp": [{"HostIp": "0.0.0.0", "HostPort": "9187"}],
        "5432/tcp": [{"HostIp": "0.0.0.0", "HostPort": "5434"}],
    });
    assert!(
        matches!(
            planned_against(&DockerHost::running(vec![Container {
                ports: both,
                ..container()
            }])),
            Plan::Launch(_),
        ),
        "a container publishing an exporter as well as PostgreSQL was not matched",
    );
}

#[test]
fn a_port_bound_on_both_ipv4_and_ipv6_is_one_candidate() {
    // Docker reports the same binding once per host address, and a Project with one
    // database has one Candidate. That there is exactly *one* is asserted where the title
    // states `N of M`; what matters here is that the repeat is still a match rather than
    // something the port reading trips over.
    let host = DockerHost::running(vec![Container {
        ports: json!({"5432/tcp": [
            {"HostIp": "0.0.0.0", "HostPort": "5434"},
            {"HostIp": "::", "HostPort": "5434"},
        ]}),
        ..container()
    }]);
    assert!(
        matches!(planned_against(&host), Plan::Launch(_)),
        "a database published on both IPv4 and IPv6 was not matched",
    );
}

#[test]
fn only_a_binding_the_dsns_own_host_can_reach_counts() {
    // A binding carries the host address it was made on, and `ports: ["192.168.1.10:5434:5432"]`
    // binds that interface and no other. The DSN says `127.0.0.1`, so a binding the loopback
    // cannot reach is not a port to put in one — resolving it produces a Pane titled after a
    // database the Client is refused by.
    let host = DockerHost::running(vec![Container {
        ports: json!({"5432/tcp": [{"HostIp": "192.168.1.10", "HostPort": "5434"}]}),
        ..container()
    }]);
    assert_eq!(
        planned_against(&host),
        found_nothing(),
        "a port bound only on a LAN address was resolved as reachable on the loopback",
    );

    // And the lowest port is the lowest of the ones that can be reached: taking the lowest
    // of all of them picks the binding the DSN cannot use, over the one it can.
    let host = DockerHost::running(vec![Container {
        ports: json!({"5432/tcp": [
            {"HostIp": "127.0.0.1", "HostPort": "5434"},
            {"HostIp": "192.168.1.10", "HostPort": "5433"},
        ]}),
        ..container()
    }]);
    assert_eq!(
        resolved(&host),
        "postgres://app@127.0.0.1:5434/orders",
        "the lowest port was taken from a binding the loopback cannot reach",
    );

    // Every address Docker binds the loopback with: the wildcards it writes for `-p 5434:5432`,
    // the loopback itself, and the empty one its API writes for an unrestricted binding.
    for host_ip in ["0.0.0.0", "127.0.0.1", "::", "::1", ""] {
        let host = DockerHost::running(vec![Container {
            ports: json!({"5432/tcp": [{"HostIp": host_ip, "HostPort": "5434"}]}),
            ..container()
        }]);
        assert!(
            matches!(planned_against(&host), Plan::Launch(_)),
            "a database bound on `{host_ip}` is reachable at 127.0.0.1 and was not matched",
        );
    }
}

#[test]
fn only_a_postgresql_image_is_a_candidate() {
    // A stack's other containers are brought up from the same compose file and publish
    // ports of their own. Launching a PostgreSQL client at Redis is a worse outcome than
    // declining, because it looks like the plugin resolved something.
    for image in ["redis:7", "nginx:1.27", "mysql:8"] {
        assert_eq!(
            planned_against(&DockerHost::running(vec![Container {
                image,
                ..container()
            }])),
            found_nothing(),
            "{image} was matched as a PostgreSQL",
        );
    }
    // The distributions that are PostgreSQL under another name, all of which take the same
    // DSN and answer the same client.
    for image in [
        "postgres:16",
        "pgvector/pgvector:pg16",
        "timescale/timescaledb:latest-pg16",
        "supabase/postgres:15.1.0",
    ] {
        assert!(
            matches!(
                planned_against(&DockerHost::running(vec![Container {
                    image,
                    ..container()
                }])),
                Plan::Launch(_),
            ),
            "{image} is a PostgreSQL and was not matched",
        );
    }
}

#[test]
fn a_container_carrying_no_compose_label_is_never_a_candidate() {
    // A hand-run `docker run postgres` cannot be attributed to any Project. It carries
    // labels — the image's own — so the test is the absence of the Compose working
    // directory and not the emptiness of the label map. Guessing which Project owns it is
    // how the wrong database gets edited.
    assert_eq!(
        planned_against(&DockerHost::running(vec![Container {
            working_dir: None,
            ..container()
        }])),
        found_nothing(),
        "a container that names no Compose directory was attributed to the Project anyway",
    );
}

#[test]
fn the_compose_directory_is_matched_component_wise_against_the_project() {
    // A compose file lives wherever the repository puts it, so the label is matched as
    // *under* the Project — which includes the Project itself, the common case of a
    // compose file at the root.
    for working_dir in [
        "/Users/b/AI/orders",
        "/Users/b/AI/orders/infra",
        "/Users/b/AI/orders/docker",
        "/Users/b/AI/orders/compose",
        "/Users/b/AI/orders/apps/api",
    ] {
        assert!(
            matches!(
                planned_against(&DockerHost::running(vec![Container {
                    working_dir: Some(working_dir),
                    ..container()
                }])),
                Plan::Launch(_),
            ),
            "a compose file at {working_dir} did not resolve for the Project it is inside",
        );
    }
    // Component-wise is the whole of it: `orders-api` shares five characters with `orders`
    // and is a different repository, whose database is a different database. Comparing the
    // paths as text matches it, and the user edits the wrong one.
    for working_dir in [
        "/Users/b/AI/orders-api",
        "/Users/b/AI/orders-api/infra",
        "/Users/b/AI",
        "/Users/b/AI/other/orders",
    ] {
        assert_eq!(
            planned_against(&DockerHost::running(vec![Container {
                working_dir: Some(working_dir),
                ..container()
            }])),
            found_nothing(),
            "a compose file at {working_dir} was treated as belonging to {PROJECT}",
        );
    }
}

#[test]
fn both_sides_are_canonicalised_before_they_are_compared() {
    // macOS reports the same directory as `/var/…` and as `/private/var/…`, and which of
    // the two a path arrives as depends on who said it: herdr names the Worktree, Compose
    // names the directory it was run from. Compared as they arrive, the same directory
    // fails to contain itself.
    let host = DockerHost::running(vec![Container {
        working_dir: Some("/private/var/AI/orders/infra"),
        ..container()
    }])
    .linking("/var", Some("/private/var"));
    assert!(
        matches!(planned_for("/var/AI/orders", &host), Plan::Launch(_)),
        "a Project named through a symlink did not contain its own compose directory",
    );

    // And the other way round: the symlinked form can just as easily be the label's.
    let host = DockerHost::running(vec![Container {
        working_dir: Some("/var/AI/orders/infra"),
        ..container()
    }])
    .linking("/var", Some("/private/var"));
    assert!(
        matches!(
            planned_for("/private/var/AI/orders", &host),
            Plan::Launch(_)
        ),
        "a compose directory named through a symlink did not resolve to the Project",
    );
}

#[test]
fn a_path_that_resolves_to_nothing_is_not_a_match() {
    // The label records where the compose file was run, on whichever machine ran it. A
    // directory that is not on this machine cannot be shown to be inside the Project, and
    // taking the unresolved path on trust would match it on its spelling — the exact thing
    // canonicalising is there to stop.
    let host = DockerHost::running(vec![container()]).linking(PROJECT, None);
    assert_eq!(
        planned_against(&host),
        found_nothing(),
        "a directory this machine does not have was matched to the Project",
    );
}

#[test]
fn docker_being_absent_or_down_is_a_silent_decline() {
    // Most machines this runs on have no Docker at all, and a stopped daemon is the normal
    // state of one that does. Neither is a fault of the Project's, so neither may be
    // classified: a Strategy that found nothing lets the chain go on to the ones that will
    // arrive after it, and the user is told what they can act on — that no connection was
    // found — rather than what Docker's stderr happened to say (AC 8).
    for docker in [Docker::Absent, Docker::DaemonDown] {
        assert_eq!(
            planned_against(&DockerHost::answering(docker)),
            found_nothing(),
            "an absent or stopped Docker was reported as something other than nothing found",
        );
    }
}

#[test]
fn never_panics_whatever_docker_says() {
    // Docker's output is another program's, and its shape is versioned by that program
    // rather than by this one. A panic in a Pane is a crash the user watches happen
    // (ADR-0004), so an answer this plugin cannot read is an answer it declines on.
    for said in [
        "",
        "not json at all",
        "{}",
        "[]",
        "[null]",
        r#"[{"Config":42}]"#,
        r#"[{"Config":{"Image":"postgres:16"}}]"#,
        r#"[{"Config":{"Image":null,"Labels":null},"NetworkSettings":null}]"#,
    ] {
        assert_eq!(
            planned_against(&DockerHost::answering(Docker::Saying(said.to_string()))),
            found_nothing(),
            "the answer {said} was not declined on",
        );
    }

    // The port shapes, stated as a container that matches on everything else — they are
    // only reachable *after* the image and the label have been read, so a payload that
    // failed to match would never get near them.
    for ports in [
        json!({"5432/tcp": "5434"}),
        json!({"5432/tcp": [{"HostPort": 99_999_999}]}),
        json!({"5432/tcp": [{"HostPort": "99999999"}]}),
    ] {
        let host = DockerHost::running(vec![Container {
            ports: ports.clone(),
            ..container()
        }]);
        assert_eq!(
            planned_against(&host),
            found_nothing(),
            "the ports {ports} were not declined on",
        );
    }
}

/// The Launch a scripted Docker plans, or a failure saying what it declined with instead.
/// Every test below states a container and then reads what it renders to.
fn launched(host: &dyn Host) -> Launch {
    match planned_against(host) {
        Plan::Launch(launch) => launch,
        Plan::Decline(diagnosis) => panic!(
            "the container was expected to be a Candidate, and declined: {}",
            diagnosis.message(),
        ),
    }
}

/// The DSN the Launch carries. It is the Client's positional argument and there is nowhere
/// else in a Launch for it to be (ADR-0002).
fn resolved(host: &dyn Host) -> String {
    launched(host).argv[1].clone()
}

#[test]
fn the_dsn_is_the_containers_own_credentials_on_its_published_host_port() {
    // `docker ps` does not carry a container's environment, so the user, the password and
    // the database name are read from the matched container's `inspect` — the running
    // container's environment, not a compose file's (no compose file is read until #5).
    let host = DockerHost::running(vec![Container {
        env: vec![
            "POSTGRES_USER=app",
            "POSTGRES_PASSWORD=hunter2",
            "POSTGRES_DB=orders",
            "PATH=/usr/bin",
        ],
        ..container()
    }]);
    // The host is the loopback by number and never by name: a machine that resolves the
    // name to `::1` first fails against a port Docker bound on IPv4 (AC 6).
    assert_eq!(
        resolved(&host),
        "postgres://app:hunter2@127.0.0.1:5434/orders"
    );
}

#[test]
fn the_images_own_defaults_fill_in_whatever_the_container_does_not_say() {
    // The image applies these itself as it starts, so a container that names nothing is not
    // a container with no credentials. `POSTGRES_DB` defaults to the *resolved* role rather
    // than to `postgres`, so naming only the user renames the database with it.
    for (env, expected) in [
        (vec![], "postgres://postgres@127.0.0.1:5434/postgres"),
        (
            vec!["POSTGRES_USER=app"],
            "postgres://app@127.0.0.1:5434/app",
        ),
        (
            vec!["POSTGRES_DB=orders"],
            "postgres://postgres@127.0.0.1:5434/orders",
        ),
        // An empty value is a variable the container did not set: Compose writes one from
        // an unset shell variable, and the image reads it as absent. The same rule the
        // Project's own tiers apply to the invocation context.
        (
            vec!["POSTGRES_USER=", "POSTGRES_DB=", "POSTGRES_PASSWORD="],
            "postgres://postgres@127.0.0.1:5434/postgres",
        ),
    ] {
        let host = DockerHost::running(vec![Container {
            env: env.clone(),
            ..container()
        }]);
        assert_eq!(
            resolved(&host),
            expected,
            "the environment {env:?} did not default the way the image defaults it",
        );
    }
}

#[test]
fn an_absent_password_is_a_dsn_without_one_rather_than_a_guessed_one() {
    // A container on the default network with trust authentication needs no password, and
    // a guessed one turns a connection that would have worked into one that is refused.
    let host = DockerHost::running(vec![Container {
        env: vec!["POSTGRES_USER=app", "POSTGRES_DB=orders"],
        ..container()
    }]);
    assert_eq!(resolved(&host), "postgres://app@127.0.0.1:5434/orders");
}

#[test]
fn the_role_and_the_password_are_percent_encoded_and_the_database_is_left_as_it_reads() {
    // A password is whatever generated it, and `p@ss/word` written in as it reads is a DSN
    // naming a different host and a different database — one the Client connects to, or
    // fails to, with nothing to say about why. The database name is the last thing in the
    // DSN and stays legible instead, bar the few characters that would re-punctuate it.
    let host = DockerHost::running(vec![Container {
        env: vec![
            "POSTGRES_USER=app@corp",
            "POSTGRES_PASSWORD=p@ss/wo:rd#1",
            "POSTGRES_DB=orders+dev",
        ],
        ..container()
    }]);
    assert_eq!(
        resolved(&host),
        "postgres://app%40corp:p%40ss%2Fwo%3Ard%231@127.0.0.1:5434/orders+dev",
    );
}

#[test]
fn a_database_name_that_would_repunctuate_the_dsn_is_encoded() {
    // `POSTGRES_DB` is whatever the compose file said, and a name written in as it reads can
    // end the path and start a query, or re-root the path at a different database — which
    // the Client connects to while the title names the one that was asked for. Only the
    // characters that punctuate a URI are encoded, so an ordinary name stays readable.
    for (entry, expected) in [
        ("POSTGRES_DB=my db", "my%20db"),
        ("POSTGRES_DB=orders/prod", "orders%2Fprod"),
        (
            "POSTGRES_DB=orders?sslmode=disable",
            "orders%3Fsslmode=disable",
        ),
        ("POSTGRES_DB=orders#1", "orders%231"),
        // `%` too: left alone it makes every encoding above ambiguous — a name really
        // containing `%2F` would arrive at the Client as one containing `/`.
        ("POSTGRES_DB=100%", "100%25"),
        // Untouched, because none of them can move the boundary between the DSN's parts.
        ("POSTGRES_DB=orders+dev", "orders+dev"),
        ("POSTGRES_DB=orders-2024_v1.a~b", "orders-2024_v1.a~b"),
    ] {
        let host = DockerHost::running(vec![Container {
            env: vec!["POSTGRES_USER=app", entry],
            ..container()
        }]);
        assert_eq!(
            resolved(&host),
            format!("postgres://app@127.0.0.1:5434/{expected}"),
            "`{entry}` was not written into the DSN safely",
        );
    }
}

#[test]
fn an_environment_entry_is_split_on_its_first_equals_only() {
    // Base64 pads with `=`, so a generated password routinely carries one. Cut at the
    // second `=`, the password is quietly wrong, and the user watches the database refuse
    // a password they can read in `docker inspect` and see is right.
    let host = DockerHost::running(vec![Container {
        env: vec![
            "POSTGRES_USER=app",
            "POSTGRES_PASSWORD=c2VjcmV0==",
            "POSTGRES_DB=orders",
        ],
        ..container()
    }]);
    assert_eq!(
        resolved(&host),
        "postgres://app:c2VjcmV0%3D%3D@127.0.0.1:5434/orders",
    );
}

#[test]
fn the_title_names_the_database_the_origin_and_the_connecting_role() {
    // The title is the only on-screen statement of what was connected to (ADR-0007), so it
    // answers the three questions the user has in front of a Client that shows none of
    // them: which database, where the answer came from, and as whom.
    assert_eq!(
        launched(&DockerHost::running(vec![container()])).title,
        "orders@5434 · docker · app",
    );
}

#[test]
fn the_title_carries_neither_the_password_nor_a_dsn() {
    // The title is on screen for as long as the Pane is, in front of whoever walks past it
    // and in whatever the user screenshots (AC 10, ADR-0005). Asserted against a scripted
    // password rather than by reading the format string, because the format string is the
    // thing that changes.
    let title = launched(&DockerHost::running(vec![Container {
        env: vec![
            "POSTGRES_USER=app",
            "POSTGRES_PASSWORD=hunter2",
            "POSTGRES_DB=orders",
        ],
        ..container()
    }]))
    .title;
    assert!(
        !title.contains("hunter2"),
        "the title shows the password: {title}"
    );
    assert!(!title.contains("://"), "the title shows a DSN: {title}");
}

#[test]
fn a_local_candidate_opens_read_write() {
    // ADR-0005: spot-editing what an agent just wrote is the point of a local database.
    // Read-only is the Override's default instead, because an Override is by construction
    // the route to something someone else may be using.
    assert!(
        !launched(&DockerHost::running(vec![container()])).read_only,
        "a container on this machine was opened read-only",
    );
}

#[test]
fn the_lowest_published_port_launches_and_the_title_says_what_it_is_one_of() {
    // Two containers under one Project is reachable now and the picker is not (#10). Until
    // it arrives the highest-ranked Candidate launches, and the title says how many there
    // were so that "this is not the one I meant" is answerable from the screen rather than
    // from `docker ps`. Scripted in the wrong order, because `docker ps` lists in whichever
    // order it lists and a rank that depended on it would differ run to run.
    let host = DockerHost::running(vec![
        Container {
            id: "c1",
            name: "/orders-cache",
            ports: published(&["5544"]),
            env: vec!["POSTGRES_DB=cache"],
            ..container()
        },
        container(),
    ]);
    let launch = launched(&host);
    assert_eq!(launch.title, "orders@5434 · docker · app · 1 of 2");
    assert_eq!(launch.argv[1], "postgres://app@127.0.0.1:5434/orders");
}

#[test]
fn two_candidates_on_one_port_are_ordered_by_the_container_name() {
    // Two stacks can publish the same port on one machine — one of them started while the
    // other was down. The port cannot separate them, so the name does: the tie-break is the
    // container's, as Docker names it and without the leading `/` it reports it with, so
    // the order is the one the user reads in `docker ps` rather than one only this plugin
    // can see.
    let host = DockerHost::running(vec![
        Container {
            id: "c1",
            name: "/orders-web",
            env: vec!["POSTGRES_USER=app", "POSTGRES_DB=web"],
            ..container()
        },
        Container {
            id: "c0",
            name: "/orders-api",
            env: vec!["POSTGRES_USER=app", "POSTGRES_DB=api"],
            ..container()
        },
    ]);
    assert_eq!(
        launched(&host).title,
        "api@5434 · docker · app · 1 of 2",
        "two containers publishing one port did not resolve by name",
    );
}

/// A verified trap, not a style preference (ADR-0004).
///
/// `plan()`'s signature already makes the mistake unreachable: neither the invocation
/// context nor the Host offers a working directory. This guards the tier below that, where
/// a Resolution Strategy could still reach for it directly.
#[test]
fn never_establishes_liveness_from_a_listening_port() {
    // Docker Desktop holds `*:5432` open with no container behind it, so something
    // answering on the port is not something to connect to. Docker reporting a container as
    // running is the only source of liveness there is, and the cheap-looking shortcut —
    // opening a socket to see if anything is there — resolves the user to a port that
    // accepts the connection and then refuses every query.
    for (path, source) in sources() {
        for shortcut in ["TcpStream", "TcpListener", "lsof", "netstat", "SocketAddr"] {
            assert!(
                !source.contains(shortcut),
                "{} names `{shortcut}`. Liveness comes from Docker reporting a container as \
                 running and never from a port answering (AC 7)",
                path.display(),
            );
        }
    }
}

#[test]
fn never_consults_the_process_working_directory() {
    for (path, source) in sources() {
        assert!(
            !source.contains("env::current_dir"),
            "{} names `env::current_dir`, which for a plugin Pane answers with this \
             plugin's own install directory (ADR-0004)",
            path.display(),
        );
    }
}
