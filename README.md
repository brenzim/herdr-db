# herdr-db

A herdr plugin (`id = "db"`) that opens a PostgreSQL browser in a herdr Pane, already
connected to the database belonging to the Project that the Worktree herdr was invoked
from identifies.

The plugin builds no database UI of its own. herdr's Pane runs any command in a real PTY
with full keyboard input, so an existing terminal Client *is* the Pane body (ADR-0001). The
plugin's job is Connection Resolution — turning a Worktree into a DSN — and it hands that
DSN to the Client and gets out of the way. Resolution is keyed on the Project, so every
Worktree of a repository resolves alike (ADR-0003).

## Status

Walking skeleton. The plugin links into herdr, binds an action, and opens the Client in a
split Pane against a **hardcoded DSN**. Connection Resolution does not exist yet.

## Requirements

- herdr 0.8.0 or later, on Linux or macOS
- a Rust toolchain (the plugin builds from source at install time)
- [lazysql](https://github.com/jorgerojas26/lazysql) on `PATH` — `brew install lazysql`

The install-time build step verifies lazysql is present and fails loudly if it is not. It
does not install lazysql for you.

## Local development loop

Link this source tree as a plugin:

```sh
herdr plugin link .
```

Then bind the action in `~/.config/herdr/config.toml` and press the key.

Two rules make the loop short, and the difference between them matters:

- **A source change needs only a rebuild.** `cargo build --release` and press the key again
  — herdr spawns the Pane's command afresh every time, so the new binary is picked up with
  no herdr-side step at all.
- **A manifest change needs `unlink` then `link`.** Whether `herdr plugin link` re-reads an
  edited `herdr-plugin.toml` in place is untested, so after editing the manifest:

  ```sh
  herdr plugin unlink db && herdr plugin link .
  ```

Run the tests with `cargo test`.

## How the Pane is opened

The `open-db` action runs `scripts/open-db.sh`, which asks herdr to open the manifest's Pane
as a split. The launcher passes **no working-directory or environment overrides** to
`herdr plugin pane open`, and neither should anything else:

- Overriding the working directory breaks the Pane's relative `command[0]` — and inside a
  built plugin source tree it silently runs *that* tree's binary instead of the installed
  one. This is verified behaviour, not a theory.
- Overriding the environment is unnecessary: herdr injects the invocation context into the
  Pane process itself, and that context — never the process working directory — is what
  identifies the Project (ADR-0004).

`tests/launcher.rs` runs the launcher against a stub herdr and fails the build if either
override appears in the argv it asks for — or anywhere in this README, so the documentation
can never recommend what the launcher refuses to do.

## Domain vocabulary

Defined in [`CONTEXT.md`](./CONTEXT.md). Decisions are recorded in [`docs/adr/`](./docs/adr/).
