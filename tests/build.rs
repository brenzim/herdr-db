//! The install-time build step. The Client is a dependency the plugin detects but does not
//! vendor (ADR-0001), so a missing Client has to be discovered at install time rather than
//! at the moment the user first presses the key.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
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

/// The directory holding the toolchain running these tests. Taken from the cargo that
/// launched them rather than searched for on PATH, so the runs that rearrange PATH can
/// still name a real cargo and a real rustc.
fn toolchain_directory() -> PathBuf {
    PathBuf::from(env!("CARGO"))
        .parent()
        .expect("the cargo binary is in a directory")
        .to_path_buf()
}

/// This machine's target triple, as cargo names its output directory for it.
fn host_triple() -> String {
    let out = Command::new(toolchain_directory().join("rustc"))
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

/// A CARGO_HOME holding no `env` file, for the runs that must keep control of PATH: sourcing
/// rustup's env file puts $HOME/.cargo/bin ahead of everything a test put there, and the build
/// step consults HOME for nothing else. The real registry is linked in because cargo resolves
/// the whole manifest — dev-dependencies included — before building anything, and an untouched
/// CARGO_HOME would send it to the network for an index it already has a copy of.
fn cargo_home_without_an_env_file(label: &str) -> PathBuf {
    let dir = scratch(label);
    let real = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .unwrap_or_default();
    let registry = real.join("registry");
    if registry.is_dir() {
        symlink(&registry, dir.join("registry")).expect("link the registry in");
    }
    dir
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

/// Point cargo at `triple` from a config file inside `tree`, which is the one route the
/// build step cannot neutralise by clearing the environment.
fn configure_cargo_target(tree: &Path, triple: &str) {
    fs::create_dir_all(tree.join(".cargo")).expect("create the cargo config directory");
    fs::write(
        tree.join(".cargo").join("config.toml"),
        format!("[build]\ntarget = \"{triple}\"\n"),
    )
    .expect("write the cargo config");
}

/// A real target triple that is definitely not this machine's, whichever machine that is.
/// Naming one outright would make the test pass vacuously on a host that happens to be it.
fn a_triple_that_is_not_this_machines() -> &'static str {
    let candidates = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"];
    let host = host_triple();
    candidates
        .into_iter()
        .find(|triple| *triple != host)
        .expect("one of the candidates differs from the host")
}

fn path_with_a_stub_client(label: &str) -> String {
    format!(
        "{}:{}",
        directory_containing_a_stub_client(label).display(),
        std::env::var("PATH").unwrap_or_default(),
    )
}

#[test]
fn replaces_a_stale_binary_rather_than_leaving_it_in_place() {
    // With a triple configured, cargo writes to target/<triple>/release and never touches
    // the manifest's path — so a binary left there by an earlier install must not be what
    // the Pane goes on running. An install that exits 0 having changed nothing is worse
    // than one that fails.
    let tree = fresh_copy_of_the_source_tree("stale-tree");
    configure_cargo_target(&tree, &host_triple());

    let binary = tree.join(pane_binary_path());
    fs::create_dir_all(binary.parent().expect("the binary has a parent"))
        .expect("create the release directory");
    fs::write(&binary, "#!/bin/sh\necho STALE\n").expect("plant a stale binary");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
        .expect("make the stale binary executable");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", path_with_a_stub_client("stale-path"))
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "the build step must succeed, but it said:\n{}",
        everything_said(&out),
    );

    let built = tree
        .join("target")
        .join(host_triple())
        .join("release")
        .join("herdr-db");
    assert_eq!(
        fs::read(&binary).expect("read the installed binary"),
        fs::read(&built).expect("read the freshly built binary"),
        "the manifest's path still holds the stale binary, so the Pane would run it",
    );
}

#[test]
fn installs_when_the_environment_has_no_home() {
    // launchd and systemd can exec with no HOME at all — the same login-less launches the
    // rustup fallback exists to serve. Reaching for $HOME unguarded under `set -u` would
    // abort the install for a user whose toolchain is already on PATH.
    let tree = fresh_copy_of_the_source_tree("no-home-tree");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", path_with_a_stub_client("no-home-path"))
        .env_remove("HOME")
        .env_remove("CARGO_HOME")
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "the build step must survive an environment with no HOME, but it said:\n{}",
        everything_said(&out),
    );
    assert!(tree.join(pane_binary_path()).is_file());
}

#[test]
fn refuses_to_install_a_build_for_a_foreign_target_triple() {
    // The safety property the host-triple recovery leans on: only a binary that runs on
    // this machine may reach the manifest's path. (On a machine without that target
    // installed the refusal comes from cargo rather than from the guard below it — either
    // way, a foreign binary must never be installed.)
    let tree = fresh_copy_of_the_source_tree("foreign-tree");
    configure_cargo_target(&tree, a_triple_that_is_not_this_machines());

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", path_with_a_stub_client("foreign-path"))
        .output()
        .expect("run the build step");
    assert!(
        !out.status.success(),
        "a foreign-triple build must not install, but the build step said:\n{}",
        everything_said(&out),
    );
    assert!(
        !tree.join(pane_binary_path()).is_file(),
        "a binary that cannot run on this machine reached the manifest's path",
    );
}

#[test]
fn installs_when_the_plugin_root_is_reached_through_a_symlink() {
    // macOS reaches /tmp through a symlink to /private/tmp, and plugin directories get
    // symlinked into place by hand too — so the shell's working directory and the physical
    // path cargo reports name the same file with two different strings. Deciding whether the
    // built binary is already the installed one by comparing those strings copies the file
    // onto itself, which fails, and takes an otherwise perfect install down with it.
    let tree = fresh_copy_of_the_source_tree("symlink-tree");
    let link = scratch("symlink-root").join("plugin");
    symlink(&tree, &link).expect("symlink the plugin root");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&link)
        // A shell keeps an inherited PWD that names its working directory, symlink and all.
        // That is what the build step sees when herdr was launched through one.
        .env("PWD", &link)
        .env("PATH", path_with_a_stub_client("symlink-path"))
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "a plugin root reached through a symlink must still install, but it said:\n{}",
        everything_said(&out),
    );

    assert!(
        link.join(pane_binary_path()).is_file(),
        "the Pane binary is missing from the path the manifest names",
    );
}

#[test]
fn reports_a_compile_error_as_a_compile_error_and_not_as_a_bug_to_report() {
    // /bin/sh has no `pipefail`, so cargo run in a pipeline hides its exit status: a build
    // that did not compile looks exactly like one that reported no artifact, and the user is
    // told to report their own compile error as a bug in this plugin.
    let tree = fresh_copy_of_the_source_tree("compile-error-tree");
    let source = tree.join("src").join("lib.rs");
    let mut broken = fs::read_to_string(&source).expect("read the source");
    broken.push_str("\npub fn deliberately_broken() -> u32 {\n    \"not a number\"\n}\n");
    fs::write(&source, broken).expect("break the source");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", path_with_a_stub_client("compile-error-path"))
        .output()
        .expect("run the build step");
    let said = everything_said(&out);

    assert!(
        !out.status.success(),
        "source that does not compile must fail the install, but it said:\n{said}",
    );
    assert!(
        said.contains("mismatched types"),
        "the failure must be the compiler's own report of the error, which is the only thing \
         that tells the user what to fix. It said:\n{said}",
    );
    assert!(
        !said.contains("Please report this"),
        "the user is being asked to file a bug against this plugin for their own compile \
         error. It said:\n{said}",
    );
}

#[test]
fn builds_for_this_machine_even_when_the_environment_names_another_target() {
    // A CARGO_BUILD_TARGET inherited from the environment herdr was launched in would
    // cross-compile: at best a binary the Pane cannot run, and on a machine without that
    // target installed no binary at all. Noticing afterwards where the artifact landed only
    // reports the damage; clearing the variable is what keeps a native build native.
    let tree = fresh_copy_of_the_source_tree("env-triple-tree");
    let path = path_with_a_stub_client("env-triple-path");

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", &path)
        .env("CARGO_BUILD_TARGET", a_triple_that_is_not_this_machines())
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "a foreign triple in the environment must not fail the install, but it said:\n{}",
        everything_said(&out),
    );

    // That the binary is this machine's is proved by this machine running it: it execs the
    // Client, which the stub on PATH answers for.
    let ran = Command::new(tree.join(pane_binary_path()))
        .env("PATH", &path)
        .status()
        .expect("run the installed Pane binary");
    assert!(
        ran.success(),
        "the installed Pane binary does not run on this machine: {ran}",
    );
}

#[test]
fn refuses_to_install_when_rustc_cannot_say_which_machine_this_is() {
    // A rustup shim with no default toolchain answers `rustc -vV` with an error, so there is
    // no host triple to compare the artifact's path against — and that comparison is the only
    // thing that tells a native `target/<triple>/release` build from a foreign one. Installing
    // anyway would drop the foreign-binary guard rather than degrade it, moving the failure to
    // the user's first keypress; refusing costs nothing, since a build with no triple in its
    // path never reaches this case at all.
    let tree = fresh_copy_of_the_source_tree("no-rustc-tree");
    configure_cargo_target(&tree, &host_triple());

    let shims = directory_containing_a_stub_client("no-rustc-path");
    let failing_rustc = shims.join("rustc");
    fs::write(
        &failing_rustc,
        "#!/bin/sh\necho 'error: no default toolchain configured' >&2\nexit 1\n",
    )
    .expect("write the failing rustc");
    fs::set_permissions(&failing_rustc, fs::Permissions::from_mode(0o755))
        .expect("make the failing rustc executable");

    // Nothing may prepend a directory ahead of the shim, or the real rustc answers and the run
    // proves nothing about the case this test is named for. rustup's env script does exactly
    // that, on every machine whose PATH does not already hold the string it looks for — so the
    // build step is given a CARGO_HOME with no env file to source. The toolchain is then named
    // on PATH outright, behind the shim, so the build still compiles on a machine that reaches
    // cargo only through the env file that is now never sourced.
    let cargo_home = cargo_home_without_an_env_file("no-rustc-cargo-home");
    let path = format!(
        "{}:{}:{}",
        shims.display(),
        toolchain_directory().display(),
        std::env::var("PATH").unwrap_or_default(),
    );

    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", &path)
        .env("CARGO_HOME", &cargo_home)
        // cargo is told where rustc really is, so the build itself still runs while the
        // build step's own `rustc -vV` finds only the failing shim ahead of it on PATH.
        .env("RUSTC", toolchain_directory().join("rustc"))
        .output()
        .expect("run the build step");
    let said = everything_said(&out);

    assert!(
        !out.status.success(),
        "a build that cannot be confirmed as this machine's must not install, but the build \
         step said:\n{said}",
    );
    assert!(
        said.contains("rustc could not say which machine this is"),
        "the run did not reach the unknown-host case at all, so it proves nothing about it \
         — the shim must be the rustc the build step finds. It said:\n{said}",
    );
    assert!(
        !tree.join(pane_binary_path()).is_file(),
        "an unconfirmed binary reached the manifest's path, so the Pane would run it",
    );
}

#[test]
fn installs_the_pane_binary_even_when_the_build_produces_other_executables() {
    // Only the Pane binary may reach the manifest's path. A second [[bin]] — or an example, or
    // a build script — is reported as an executable of this build just as the Pane binary is,
    // and installing one of those would hand the Pane a program that is not the Pane.
    let tree = fresh_copy_of_the_source_tree("two-bins-tree");
    let manifest = tree.join("Cargo.toml");
    let mut declared = fs::read_to_string(&manifest).expect("read the manifest");
    declared.push_str("\n[[bin]]\nname = \"other-tool\"\npath = \"src/other_tool.rs\"\n");
    fs::write(&manifest, declared).expect("declare a second binary");

    // The second binary only has to be distinguishable from the Pane binary when run, which
    // an exit status nothing else produces is enough for. Nothing here depends on the order
    // cargo reports the two in: the build step picks by name, not by position.
    fs::write(
        tree.join("src").join("other_tool.rs"),
        "fn main() {\n    std::process::exit(97);\n}\n",
    )
    .expect("write the second binary");

    let path = path_with_a_stub_client("two-bins-path");
    let out = Command::new("/bin/sh")
        .arg("scripts/build.sh")
        .current_dir(&tree)
        .env("PATH", &path)
        .output()
        .expect("run the build step");
    assert!(
        out.status.success(),
        "a build with a second binary in it must still install, but it said:\n{}",
        everything_said(&out),
    );

    // The Pane binary execs the Client and exits with its status; the other one exits 97.
    let ran = Command::new(tree.join(pane_binary_path()))
        .env("PATH", &path)
        .status()
        .expect("run the installed Pane binary");
    assert!(
        ran.success(),
        "the manifest's path holds the other binary the build produced, so the Pane would \
         run that instead of the Client: {ran}",
    );
}
