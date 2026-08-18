#!/usr/bin/env bash
# Launcher for the `open-db` action: asks herdr to open the plugin's Pane as a split.
#
# It deliberately passes no working-directory or environment overrides. herdr resolves the
# Pane's relative command against the plugin root, and supplies the invocation context to
# the Pane process itself — so overriding either would break the command or the context
# (see the README and ADR-0004).
set -eu

herdr_bin="${HERDR_BIN_PATH:-herdr}"

exec "$herdr_bin" plugin pane open \
  --plugin db \
  --entrypoint db \
  --placement split \
  --direction right \
  --focus
