//! The Host port. Nothing below the seam calls it yet — the port exists so that the
//! Resolution Strategies of #4 can be driven from a test with neither Docker nor a live
//! PostgreSQL server. What is worth pinning now is its degradation contract: every method
//! answers with an absence rather than an error, so a Decline is classified from what is
//! missing and never from the text of an io failure.

mod common;

use std::path::PathBuf;

use common::plugin_root;
use herdr_db::host::{Host, RealHost};

/// An empty scratch directory of this test's own, so no run can be satisfied by something
/// an earlier one left behind.
fn scratch(label: &str) -> PathBuf {
    let dir = plugin_root().join("target").join("host-test").join(label);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

#[test]
fn the_host_reads_a_file_and_answers_nothing_for_one_it_cannot() {
    let dir = scratch("read-file");
    let present = dir.join("present");
    std::fs::write(&present, "contents").expect("write the file");

    assert_eq!(
        RealHost.read_file(&present),
        Some("contents".to_string()),
        "a readable file must be read",
    );
    assert_eq!(
        RealHost.read_file(&dir.join("absent")),
        None,
        "a file that is not there is an absence, not a failure",
    );
}

#[test]
fn the_host_lists_a_directory_and_answers_emptily_for_one_it_cannot() {
    let dir = scratch("list-dir");
    std::fs::write(dir.join("entry"), "").expect("write the entry");

    assert_eq!(RealHost.list_dir(&dir), vec![dir.join("entry")]);
    assert_eq!(
        RealHost.list_dir(&dir.join("absent")),
        Vec::<PathBuf>::new(),
        "a directory that is not there lists as empty rather than failing",
    );
}

#[test]
fn the_host_canonicalises_a_path_through_its_symlinks() {
    let dir = scratch("canonicalize");
    let real = dir.join("real");
    std::fs::create_dir(&real).expect("create the directory the link points at");
    let link = dir.join("link");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");

    assert_eq!(
        RealHost.canonicalize(&link),
        RealHost.canonicalize(&real),
        "a symlink and its target are the same directory, and matching a container to a \
         Project compares the two",
    );
    assert_eq!(
        RealHost.canonicalize(&dir.join("absent")),
        None,
        "a path that is not there cannot be resolved, and that is an absence rather than \
         a failure",
    );
}

#[test]
fn the_host_runs_a_command_and_answers_nothing_for_one_it_cannot_run() {
    let dir = scratch("run");

    let ran = RealHost
        .run(
            "/bin/sh",
            &["-c", "echo said; echo complained >&2; exit 3"],
            &dir,
        )
        .expect("a runnable command answers");
    assert_eq!(
        (ran.status, ran.stdout.trim(), ran.stderr.trim()),
        (3, "said", "complained"),
        "a command that ran and failed is an answer, with its status — not an absence",
    );

    assert_eq!(
        RealHost.run("no-such-program-exists-here", &[], &dir),
        None,
        "a command that could not be run at all is an absence",
    );
}
