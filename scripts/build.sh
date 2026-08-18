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
  echo "herdr-db: lazysql is not installed.

herdr-db opens lazysql as the database browser; it does not install it for you.
Install it and then install this plugin again:

    brew install lazysql          # macOS / Linuxbrew
    go install github.com/jorgerojas26/lazysql@latest
" >&2
  exit 1
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
cargo build --release --target-dir target

# Reachable when `[build] target` is set in a cargo config file rather than the environment
# — the unset above cannot see that, so say what it is instead of failing opaquely. The
# artifact is deliberately not copied into place: a genuine cross-compile would put a
# foreign-architecture binary where the Pane expects a runnable one.
binary="target/release/herdr-db"
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
