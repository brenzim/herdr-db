#!/bin/sh
# Install-time build step, run by herdr from the plugin root.
#
# The Client is checked BEFORE the compile, so a machine without it fails in a second
# rather than after a full release build. The check uses only shell builtins — no `cat`,
# no external command — so it still reports properly on a PATH with nothing on it.
#
# The Client's name is spelled here as well as in src/client.rs. That is the one place the
# duplication is unavoidable: this script runs before anything Rust-side is compiled.
set -eu

if ! command -v lazysql >/dev/null 2>&1; then
  echo "herdr-db: lazysql was not found on PATH.

PATH searched: ${PATH:-(unset)}

herdr-db opens lazysql as the database browser; it does not install it for you.
If it is installed somewhere not listed above, add that directory to PATH. Otherwise
install it and then install this plugin again:

    brew install lazysql          # macOS / Linuxbrew
    go install github.com/jorgerojas26/lazysql@latest
" >&2
  exit 1
fi

# Source rustup's env if it is there, so cargo is found even when herdr was launched
# without ~/.cargo/bin on PATH (a GUI or login-less launch). rustup edits shell rc files
# only, so a perfectly working toolchain is invisible here otherwise. Written as an `if`
# rather than `[ -f ] && .` so a missing env file cannot trip `set -e`, and every expansion
# defaulted, because the same login-less environments may carry no HOME at all.
cargo_env="${CARGO_HOME:-${HOME:-}/.cargo}/env"
if [ -f "$cargo_env" ]; then
  # `set -u` is lifted across the source: rustup's env script is not written to be
  # nounset-clean (it expands $HOME unguarded), and it is not ours to fix.
  set +u
  . "$cargo_env"
  set -u
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "herdr-db: no Rust toolchain found (cargo is not on PATH).

herdr-db is built from source at install time. Install a toolchain and then install this
plugin again:

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
" >&2
  exit 1
fi

# The manifest hardcodes the Pane command as ./target/release/herdr-db, so the build output
# has to land exactly there. Cargo offers two ways for the environment to move it, and both
# are neutralised here rather than left to whatever the user's machine is configured for:
#
#   CARGO_TARGET_DIR / [build] target-dir  moves the directory  -> pinned with --target-dir
#   CARGO_BUILD_TARGET / [build] target    inserts a triple     -> unset below
#
# Cross-compiling is never right for this binary in any case: it runs on the machine that
# installed it. Without the unset, a routine cross-compilation setup builds successfully to
# target/<triple>/release and the install fails with a diagnostic it cannot act on.
unset CARGO_BUILD_TARGET

# Removed before the build, not merely overwritten by it: with a triple configured, cargo
# writes to target/<triple>/release and never touches this path, so an earlier install's
# binary would otherwise survive and the Pane would go on running it while this install
# reported success. After this, anything at this path came from this run.
binary="target/release/herdr-db"
rm -f "$binary"

cargo build --release --target-dir target

# A config file can set `[build] target`, which the unset above cannot reach. When that
# triple is the host's, the binary is native and merely misplaced — move it into place
# rather than refusing an install that is perfectly good. Only a genuine cross-compile,
# whose binary would not run here, falls through to the failure below.
if [ ! -x "$binary" ]; then
  host="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
  if [ -n "$host" ] && [ -x "target/$host/release/herdr-db" ]; then
    mkdir -p target/release
    cp "target/$host/release/herdr-db" "$binary"
  fi
fi

if [ ! -x "$binary" ]; then
  echo "herdr-db: the build finished but produced no binary at $binary." >&2
  echo "" >&2
  if [ -n "$(ls -d target/*/release 2>/dev/null || true)" ]; then
    echo "It was built for a specific target triple instead:" >&2
    ls -d target/*/release >&2
    echo "" >&2
    echo "This plugin must build for the machine it runs on. Remove the \`[build] target\`" >&2
    echo "setting from your cargo config, or install this plugin with it overridden." >&2
  else
    echo "The Pane cannot start without it. Please report this with the output above." >&2
  fi
  exit 1
fi
