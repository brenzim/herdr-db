//! The plan seam: the one boundary the whole plugin is tested at.
//!
//! Every test here drives `plan()` and asserts on the returned `Plan`, never on which
//! Resolution Strategy ran — there are none yet, and when there are, these tests must not
//! notice. What a `Plan` says is the behaviour; how it was arrived at is not.

mod common;

use std::path::{Path, PathBuf};

use common::sources;
use herdr_db::context::InvocationContext;
use herdr_db::host::{Host, Output};
use herdr_db::plan::{Diagnosis, Plan, plan};

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
    // inside it. With no Resolution Strategy built yet, an identified Project can only
    // decline for want of a connection, and that Decline names the Project it looked for.
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
    // chain has already settled on the empty root, yields the filesystem root instead —
    // which resolves every Project to `/` and looks entirely successful while doing it.
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

/// A verified trap, not a style preference: a plugin Pane's process starts in the *plugin's
/// install directory*, not in the Worktree the user invoked it from (ADR-0004). Asking the
/// process where it is would therefore resolve every Project to this plugin — confidently,
/// and with no symptom other than the wrong database.
///
/// `plan()`'s signature already makes the mistake unreachable: neither the invocation
/// context nor the Host offers a working directory. This guards the tier below that, where
/// a Resolution Strategy could still reach for it directly.
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
