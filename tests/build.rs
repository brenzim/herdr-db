//! The install-time build step. The Client is a dependency the plugin detects but does not
//! vendor (ADR-0001), so a missing Client has to be discovered at install time rather than
//! at the moment the user first presses the key.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use herdr_db::client;
use toml::Value;

/// The path the manifest tells herdr the Pane binary lives at, relative to the plugin root.
fn pane_binary_path() -> PathBuf {
    let manifest = toml::from_str::<Value>(include_str!("../herdr-plugin.toml"))
        .expect("herdr-plugin.toml is valid TOML");
    let declared = manifest["panes"][0]["command"][0]
        .as_str()
        .expect("the Pane declares a command")
        .to_string();
    PathBuf::from(declared.strip_prefix("./").unwrap_or(&declared))
}

/// This machine's target triple, as cargo names its output directory for it.
fn host_triple() -> String {
    let out = Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("ask rustc for the host triple");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc reports a host triple")
        .to_string()
}

fn plugin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_step() -> Command {
    let mut step = Command::new("/bin/sh");
    step.arg("scripts/build.sh").current_dir(plugin_root());
    step
}

/// An empty scratch directory of this test's own, so concurrently running tests never
/// share one and no run can be satisfied by an artifact an earlier run left behind.
fn scratch(label: &str) -> PathBuf {
    let dir = plugin_root().join("target").join("build-test").join(label);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

/// A copy of the plugin's source tree, without its `target/` directory. Building here
/// rather than in place means the build step can be driven for real without contending
/// with the lock the surrounding `cargo test` run holds on the crate's own target dir.
fn fresh_copy_of_the_source_tree(label: &str) -> PathBuf {
    let tree = scratch(label);
    for item in ["Cargo.toml", "Cargo.lock", "src", "scripts"] {
        let copied = Command::new("cp")
            .arg("-R")
            .arg(plugin_root().join(item))
            .arg(&tree)
            .status()
            .expect("copy the source tree");
        assert!(copied.success(), "failed to copy {item}");
    }
    tree
}

/// A directory holding an executable stub named after the Client, for the runs that need
/// the install-time check to pass without depending on what this machine has installed.
fn directory_containing_a_stub_client(label: &str) -> PathBuf {
    let dir = scratch(label);
    let stub = dir.join(client::PROGRAM);
    fs::write(&stub, "#!/bin/sh\nexit 0\n").expect("write the stub Client");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))
        .expect("make the stub Client executable");
    dir
}

fn everything_said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    )
}

#[test]
fn refuses_to_install_when_the_client_is_missing_and_says_how_to_install_it() {
    // An empty PATH is how the build step sees a machine with no Client installed. Its
    // failure path uses only shell builtins, so it still reports properly from here.
    let out = build_step()
        .env("PATH", "")
        .output()
        .expect("run the build step");
    let said = everything_said(&out);

    assert!(
        !out.status.success(),
        "the build step must fail when the Client is missing, but it succeeded saying:\n{said}",
    );
    assert!(
        said.contains(client::PROGRAM),
        "the failure must name the missing Client, but it said:\n{said}",
    );
    assert!(
        said.contains("brew install"),
        "the failure must name how to install the Client, but it said:\n{said}",
    );
}

#[test]
fn compiles_the_pane_binary_to_the_path_the_manifest_names() {
    let tree = fresh_copy_of_the_source_tree("compile-tree");
    let stubs = directory_containing_a_stub_client("compile-path");
    let path = format!(
        "{}:{}",
        stubs.display(),
        std::env::var("PATH").unwrap_or_default(),
    );

    // Neither of cargo's two ways to relocate build output may move the binary: the
    // manifest names one path, so a build that honoured either would exit 0 having put the
    // binary somewhere the Pane will never look — failing at the first keypress instead of
    // at install time, which is the whole failure this build step exists to prevent.
    // CARGO_TARGET_DIR moves the directory; CARGO_BUILD_TARGET inserts a target triple.
    let elsewhere = scratch("compile-decoy");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", path)
        .env("CARGO_TARGET_DIR", &elsewhere)
        .env("CARGO_BUILD_TARGET", host_triple())
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "the build step must succeed when the Client is present, but it said:\n{}",
        everything_said(&out),
    );

    let binary = tree.join(pane_binary_path());
    assert!(
        binary.is_file(),
        "the build step must produce the Pane binary at the path the manifest names, {}",
        binary.display(),
    );
}

#[test]
fn refuses_to_install_when_the_rust_toolchain_is_missing_and_says_so() {
    // Same shape as the missing Client, one dependency lower: without this the install
    // fails with a raw `not found` from the shell rather than a message the user can act
    // on. The Client is present here, so the run gets past that check to this one.
    let stubs = directory_containing_a_stub_client("no-cargo-path");
    // A HOME with no rustup env file, so the build step's fallback for a GUI-launched
    // herdr finds nothing either — otherwise this machine's own toolchain answers.
    let toolchainless_home = scratch("no-cargo-home");
    let out = build_step()
        .env("PATH", stubs)
        .env("HOME", &toolchainless_home)
        .output()
        .expect("run the build step");
    let said = everything_said(&out);

    assert!(
        !out.status.success(),
        "the build step must fail without a Rust toolchain, but it succeeded saying:\n{said}",
    );
    assert!(
        said.contains("herdr-db:") && said.contains("rustup"),
        "the failure must be the plugin's own message naming how to get a toolchain — a \
         raw `cargo: not found` from the shell mentions cargo too, and is exactly what this \
         check exists to replace. It said:\n{said}",
    );
}

#[test]
fn compiles_to_the_manifests_path_even_when_cargo_is_configured_for_the_host_triple() {
    // Pinning `[build] target` to the host triple is a routine cargo config — stable
    // artifact paths, sccache and RUSTFLAGS isolation — and it is not a cross-compile: the
    // binary it produces is native and runnable. It lands under a triple directory though,
    // which no environment change can prevent, since a config file is not the environment.
    let tree = fresh_copy_of_the_source_tree("host-triple-tree");
    let stubs = directory_containing_a_stub_client("host-triple-path");
    let path = format!(
        "{}:{}",
        stubs.display(),
        std::env::var("PATH").unwrap_or_default(),
    );

    fs::create_dir_all(tree.join(".cargo")).expect("create the cargo config directory");
    fs::write(
        tree.join(".cargo").join("config.toml"),
        format!("[build]\ntarget = \"{}\"\n", host_triple()),
    )
    .expect("write the cargo config");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", path)
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "a host-triple build is native and must install, but the build step said:\n{}",
        everything_said(&out),
    );

    let binary = tree.join(pane_binary_path());
    assert!(
        binary.is_file(),
        "the native binary must end up at the path the manifest names, {}",
        binary.display(),
    );
}
