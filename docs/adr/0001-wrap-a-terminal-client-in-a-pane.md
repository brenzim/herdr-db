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

Detection was presence only until the plugin came to depend on `-read-only`, which
[ADR-0005](./0005-overrides-are-machine-local-and-the-only-route-off-localhost.md) makes the
default for every Override. That was the trigger this ADR named, and the remedy is the one it
named too: the build step checks at install time rather than the plugin gaining a runtime
fallback — a Pane has already `exec`ed the Client and cannot diagnose it.

The check asks the Client which options it has, rather than comparing a version number against
a floor. The flag is the fact that matters, a release that renames or renumbers itself still
answers it, and no release history has to be tracked here. Because the Client's usage text is
not a contract, the check refuses only on a positive answer — options listed, this one absent —
and treats silence or an unfamiliar format as unconfirmable and installs anyway.
