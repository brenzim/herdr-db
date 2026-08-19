//! Naming the Pane: the argv herdr is asked to rename it with, and where in `main` that
//! ask has to happen.
//!
//! The argv is testable and is tested here. Running it is `main`'s, so the rest of this
//! file guards `src/main.rs` in source — the way `tests/diagnosis.rs` guards it against
//! terminating itself — because the ordering it has to keep is only observable in a real
//! Pane that has already `exec`ed.

use herdr_db::pane::{PANE_ID_VAR, rename_args};

/// A pane id in herdr's own shape, and a title in the shape `Candidate::title` renders.
const PANE_ID: &str = "p-7f3a";
const TITLE: &str = "orders@5434 · docker · postgres · 1 of 2";

#[test]
fn asks_herdr_to_rename_the_pane() {
    // `rename` and not `report-metadata`: the plugin-facing route rejects `--source` and
    // `--title` in both spaced and `=` forms on herdr 0.8.0, and the `label` `rename`
    // writes is a durable field separate from the terminal title — which is what makes it
    // survive the Client writing a title of its own.
    assert_eq!(
        rename_args(PANE_ID, TITLE),
        vec!["pane", "rename", PANE_ID, TITLE],
    );
}

#[test]
fn the_pane_id_precedes_every_flag() {
    // A verified trap: `rename <ID> --clear` parses and `rename --clear <ID>` does not.
    // Nothing here clears a label, so the property is stated with a title that reads like
    // a flag — whatever follows the id, the id is in front of it.
    let args = rename_args(PANE_ID, "--clear");
    let id = args
        .iter()
        .position(|arg| arg == PANE_ID)
        .expect("the argv must name the Pane it renames");

    for (index, arg) in args.iter().enumerate() {
        assert!(
            !arg.starts_with('-') || index > id,
            "`{arg}` is at {index}, in front of the pane id at {id}. herdr only parses a \
             flag after the id, so an argv that leads with one renames nothing",
        );
    }
}

#[test]
fn the_title_travels_as_one_argument() {
    let args = rename_args(PANE_ID, TITLE);

    // The title carries spaces and middle dots, and it is handed to a process rather than
    // to a shell: quoting it or splitting it on the spaces would name the Pane after the
    // first word of the title and hand herdr the rest as arguments it does not know.
    assert_eq!(
        args.iter().filter(|arg| arg.as_str() == TITLE).count(),
        1,
        "the title is not one argv element of its own: {args:?}",
    );
    assert_eq!(args.len(), 4, "the title was split: {args:?}");
}

/// The whole reason this is a slice of its own. After `exec` this process *is* the Client
/// (ADR-0001), and `exec` only returns on failure — so a rename written below it is a
/// rename that never runs on the path that matters. Guarded in source because the only
/// other way to see it is a real Pane.
#[test]
fn applies_the_title_before_execing_the_client() {
    let main = include_str!("../src/main.rs");
    let rename = main
        .find("rename_args")
        .expect("`src/main.rs` must ask herdr to rename the Pane");
    let exec = main
        .find(".exec()")
        .expect("`src/main.rs` must become the Client");

    assert!(
        rename < exec,
        "`src/main.rs` renames the Pane after `exec`ing the Client. `exec` returns only on \
         failure, so the title would be set on no successful launch at all — keep the \
         rename in front of the `exec` in `launch_client`",
    );
}

#[test]
fn a_pane_that_was_given_no_id_still_launches_the_client() {
    let main = include_str!("../src/main.rs");
    let reads: Vec<&str> = main
        .lines()
        .map(str::trim)
        .filter(|line| line.contains("PANE_ID_VAR"))
        .collect();
    assert!(
        !reads.is_empty(),
        "`src/main.rs` never reads `{PANE_ID_VAR}`, so herdr's Pane is never named",
    );

    // A binary someone ran by hand is given no pane id, and that is not a fault: there is
    // no Pane to name and the Client still has to start.
    for line in reads {
        for terminator in ["unwrap", "expect"] {
            assert!(
                !line.contains(terminator),
                "`src/main.rs` `{terminator}`s the pane id ({line}). A Pane id that is not \
                 there is a plugin run outside herdr — skip the rename and launch",
            );
        }
    }
}

/// Renaming the Pane is one of the four things the binary does *above* the seam, alongside
/// drawing the picker, `exec`ing the Client and holding the diagnosis open. `Host` is what
/// Connection Resolution may consult; routing the rename through it would put a herdr call
/// inside the port every Strategy is driven from in tests.
#[test]
fn does_not_route_the_rename_through_the_host_port() {
    let main = include_str!("../src/main.rs");
    assert!(
        !main.contains("host.run("),
        "`src/main.rs` runs a command through the Host port. The Host is what Connection \
         Resolution consults; what `main` does above the seam it does directly",
    );

    let pane = include_str!("../src/pane.rs");
    for below in ["Host", "Command"] {
        assert!(
            !pane.contains(below),
            "`src/pane.rs` names `{below}`. It states the argv and runs nothing, so it \
             stays testable without a herdr to answer",
        );
    }
}
