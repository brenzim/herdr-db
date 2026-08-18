# Wrap a terminal database client rather than build a database UI

herdr's `[[panes]]` entrypoint runs any argv command in a real PTY with full keyboard input,
which means an existing terminal database client can *be* the pane body rather than something
we render around. We therefore build no database UI of our own: the plugin's entire job is
Connection Resolution, and it hands the resulting DSN to a Client we did not write.

## Consequences

The plugin owns no grid, no query editor, no result rendering, and inherits the Client's
feature set and its bugs wholesale. In exchange, essentially all of our code and all of our
testing effort concentrates on resolution, which is the part that is actually specific to us.
The Client becomes a dependency we must detect, not vendor: the install-time build step
fails when it is absent from `PATH`.

Detection is presence only — there is no minimum-version check. The integration surface is a
single positional DSN ([ADR-0002](./0002-lazysql-as-the-client.md)), which every released
lazysql has accepted, so there is no version-sensitive behaviour to guard and no floor worth
naming. The trigger to revisit is the plugin coming to depend on a flag or behaviour that
arrived in a particular release — `-read-only` being the first candidate — at which point the
build step gains a version check rather than the plugin gaining a runtime fallback.
