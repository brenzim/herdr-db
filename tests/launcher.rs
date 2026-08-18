//! The launcher is what the `open-db` action runs. Its whole job is to ask herdr to open
//! the plugin's Pane.
//!
//! These drive the script for real, with `HERDR_BIN_PATH` pointed at a stub that records
//! the argv it was called with — so they assert what the launcher *asks herdr to do*, and
//! survive any rewrite of the script that keeps asking for the same thing.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use toml::Value;

fn plugin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run the launcher against a stub herdr, and return the argv the launcher invoked it with.
/// `label` scopes the recording to one test, since tests in a binary run concurrently.
fn herdr_invocation(label: &str) -> Vec<String> {
    let scratch = plugin_root()
        .join("target")
        .join("launcher-test")
        .join(label);
    fs::create_dir_all(&scratch).expect("create the stub directory");

    let recording = scratch.join("argv");
    let _ = fs::remove_file(&recording);

    // The stub takes the recording path from the environment rather than from its own source,
    // so a path containing shell metacharacters cannot corrupt the script.
    let stub = scratch.join("herdr-stub");
    fs::write(
        &stub,
        "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done > \"$HERDR_DB_TEST_RECORDING\"\n",
    )
    .expect("write the stub");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))
        .expect("make the stub executable");

    // `open-db.sh` execs the stub, so the stub inherits this environment.
    let run = Command::new("bash")
        .arg("scripts/open-db.sh")
        .current_dir(plugin_root())
        .env("HERDR_BIN_PATH", &stub)
        .env("HERDR_DB_TEST_RECORDING", &recording)
        .output()
        .expect("run the launcher");
    assert!(
        run.status.success(),
        "the launcher failed: {}",
        String::from_utf8_lossy(&run.stderr),
    );

    fs::read_to_string(&recording)
        .expect("the launcher must invoke herdr")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether `text` names `flag` *as that flag*, rather than merely as the prefix of a longer
/// flag name: `--env` is named by `--env` and `--env=x`, but not by `--env-file`.
fn names_flag(text: &str, flag: &str) -> bool {
    text.match_indices(flag).any(|(at, _)| {
        text[at + flag.len()..]
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '-' && next != '_')
    })
}

/// The value the launcher passed for `flag`, if it passed one.
fn value_of<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let at = argv.iter().position(|a| a == flag)?;
    argv.get(at + 1).map(String::as_str)
}

#[test]
fn asks_herdr_to_open_the_plugins_pane_as_a_split() {
    let argv = herdr_invocation("split");
    assert_eq!(
        argv.first().map(String::as_str),
        Some("plugin"),
        "the launcher must drive herdr's plugin CLI, but it ran: {argv:?}",
    );
    assert!(
        argv.windows(2).any(|w| w == ["pane", "open"]),
        "the launcher must open a Pane, but it ran: {argv:?}",
    );
    assert_eq!(value_of(&argv, "--placement"), Some("split"));
}

#[test]
fn opens_the_pane_this_plugin_actually_declares() {
    // Nothing else ties the launcher's ids to the manifest's, so a renamed Pane would
    // otherwise leave a launcher that opens nothing.
    let manifest = toml::from_str::<Value>(include_str!("../herdr-plugin.toml"))
        .expect("herdr-plugin.toml is valid TOML");
    let argv = herdr_invocation("ids");

    assert_eq!(value_of(&argv, "--plugin"), manifest["id"].as_str());
    assert_eq!(
        value_of(&argv, "--entrypoint"),
        manifest["panes"][0]["id"].as_str(),
    );
}

/// A verified trap, not a style preference: passing `--cwd` to `herdr plugin pane open`
/// breaks the Pane's relative `command[0]`, and inside a built plugin source tree it
/// silently runs *that* tree's binary. The invocation context supplies everything the Pane
/// needs (ADR-0004), so neither override is ever warranted.
///
/// The README is guarded alongside the launcher so the documentation can never recommend
/// what the launcher refuses to do.
#[test]
fn passes_neither_a_working_directory_nor_an_environment_override() {
    let argv = herdr_invocation("overrides");
    for flag in ["--cwd", "--env"] {
        assert!(
            !argv.iter().any(|a| names_flag(a, flag)),
            "the launcher passed `{flag}`: it breaks the Pane's relative command and can \
             silently launch another source tree's binary. It ran: {argv:?}",
        );
        assert!(
            !names_flag(include_str!("../README.md"), flag),
            "the README names `{flag}`, so the documentation can come to recommend what \
             the launcher refuses to do",
        );
    }
}
