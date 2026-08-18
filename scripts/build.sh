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
  # `${PATH-...}` without the colon so a set-but-empty PATH is not reported as unset —
  # a sanitised launch environment usually exports an empty PATH rather than dropping it.
  path_searched="${PATH-(unset)}"
  if [ -z "$path_searched" ]; then
    path_searched="(empty)"
  fi
  echo "herdr-db: lazysql was not found on PATH.

PATH searched: $path_searched

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
# has to end up exactly there. Rather than predict where cargo will write — CARGO_TARGET_DIR
# moves the directory, CARGO_BUILD_TARGET or a config file's `[build] target` inserts a
# triple — the build is asked to report the artifact it produced, and that exact file is
# installed. Asking removes every way a previous run's leftovers could be mistaken for this
# one's output, and means nothing has to be deleted up front: a failed build leaves the
# currently installed binary untouched rather than destroying it.
#
# `json-render-diagnostics` keeps compiler errors human-readable on stderr while the machine
# format goes to stdout. The path is reported even when the build is a no-op.
binary="target/release/herdr-db"

built="$(
  cargo build --release --target-dir target --message-format=json-render-diagnostics \
    | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' \
    | tail -1
)"

if [ -z "$built" ] || [ ! -x "$built" ]; then
  echo "herdr-db: the build reported no executable. The Pane cannot start without one." >&2
  echo "Please report this with the output above." >&2
  exit 1
fi

# A triple in the path means cargo was configured to build for a specific target. That is
# fine when it is this machine's — a common config for stable artifact paths — and the
# binary is simply somewhere the manifest does not name. It is not fine when it is another
# machine's, because that binary cannot run here.
host="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
case "$built" in
  */target/release/herdr-db) ;;
  */target/"$host"/release/herdr-db) ;;
  *)
    echo "herdr-db: the build produced a binary for another machine:" >&2
    echo "" >&2
    echo "    $built" >&2
    echo "" >&2
    echo "This plugin must build for the machine it runs on. Remove the \`[build] target\`" >&2
    echo "setting from your cargo config, or install this plugin with it overridden." >&2
    exit 1
    ;;
esac

if [ "$built" != "$PWD/$binary" ]; then
  mkdir -p target/release
  cp "$built" "$binary"
fi
