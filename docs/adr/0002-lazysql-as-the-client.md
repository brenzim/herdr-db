# lazysql as the Client

We evaluated lazysql against rainfrog. rainfrog treats PostgreSQL as a tier-one database and
has the better query editor, but it cannot edit rows in its results grid and holds a single
connection at a time. The intended use is inspecting and spot-editing records an agent has
just written, across several Projects at once — so lazysql's tabs, in-grid row editing, and
per-table constraint/foreign-key/index panes decide it.

## Consequences

We accept a weaker query editor as the cost of in-grid editing. lazysql takes the DSN as a
positional argument, `lazysql <dsn>` (confirmed in its `main.go`), plus `-read-only` and `-config`,
so no generated config file is needed and the DSN is the whole integration surface — which is
what keeps [ADR-0001](./0001-wrap-a-terminal-client-in-a-pane.md) cheap and keeps the Client
swappable if this judgement ages badly.

A positional DSN is an argv, and argv is world-readable in the process table: any user on the
host can read the DSN, credentials included, out of `ps` while a Pane is open. This is a
different exposure from the one
[ADR-0005](./0005-overrides-are-machine-local-and-the-only-route-off-localhost.md) addresses —
redaction covers what the plugin *displays and logs*, and cannot cover what it *passes*. It is
accepted for local Candidates, whose credentials are already in world-readable compose files on
the same machine. It is worth reopening when an Override carries a credential that is not
otherwise on this host, since lazysql's only alternative surface is `-config`, which trades the
process table for a file on disk rather than removing the exposure.
