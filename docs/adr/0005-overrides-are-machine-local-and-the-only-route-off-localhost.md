# Overrides are machine-local, and are the only route to a non-local database

Resolution Strategies look only at local, mechanically-derivable things. A database on a remote
test host is reachable *only* by writing an Override — never by inference. Overrides live in a
single machine-local file under herdr's plugin config directory
(`$HERDR_PLUGIN_CONFIG_DIR`), keyed by Project, so no existing repository has to be modified for
the plugin to be useful. A repo-local Override source is anticipated as a second, higher-priority
source of the *same format*, but is not built yet.

## Considered Options

Auto-discovering remote hosts — from non-localhost values in `.env`, from tunnel scripts, from
Maven filter properties — was rejected. A local container is disposable; a shared test host is
not, and is very likely being used by someone else. Silently resolving to one is how the wrong
row gets edited. Requiring a deliberate human act is the point, not a limitation.

## Consequences

The plugin never derives or stores a credential of its own: local passwords are lifted from
compose files that already hold them in plaintext, and a remote password, if needed, is
something the user places in the Override — which is outside every repository and therefore
safe from being committed. herdr offers no secret storage of any kind, so there is no better
option available. Because real credentials are in scope, every DSN is redacted wherever it is
displayed or logged; a future debugging change that logs a raw DSN is a security regression.

An Override is keyed on the Project's absolute path rather than on the opaque `repo_key` herdr
also supplies. `repo_key` would survive moving a directory, but its stability semantics are
undocumented, and an escape hatch the user cannot read and edit with confidence is a bad escape
hatch. Overrides carry `dsn`, `label`, and `read_only`, and default to `read_only = true`: local
Candidates open read-write because spot-editing what an agent just wrote is the point, whereas
an Override is by construction the route to something the Strategies deliberately refuse to
infer, which correlates with "someone else may be using this".

An Override may declare several named connections for one Project, each with its own `dsn`,
`label`, and `read_only`. They enter the picker as additional Candidates
([ADR-0007](./0007-pick-when-ambiguous-announce-always.md)). This exists because a single
Postgres service routinely backs several logical databases that no Strategy can see — one
surveyed project runs three (`kratos`, `hydra`, `keto`) behind one service, none of them named
by the compose file.
