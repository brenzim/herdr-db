# The Strategy chain stops at declarations Compose renders

Connection Resolution consults an Override, then the running Docker containers, then Compose's own
configuration renderer — and stops there. A Stack whose render exits non-zero is diagnosed and
never read approximately: no direct parse of the files the renderer refused, no widening of the
filenames Compose loads, and no port recovered from either. The escape hatch is an Override
([ADR-0005](./0005-overrides-are-machine-local-and-the-only-route-off-localhost.md)).

## Why

The originating issue described a fourth Strategy — "a tolerant direct read for the files Compose
itself refuses to resolve". Two separate things removed it.

**[ADR-0008](./0008-declaration-supplies-identity-never-a-port.md) removed what it could return.**
A declaration supplies an identity and never a port, so an approximate read of a file Compose would
not render cannot produce a DSN of its own: it can only name a service and hand the connection back
to a running container. Where a container is running, the Live Docker and Compose Strategies already
reach it. Where none is, the read yields a Decline whatever it manages to parse. A Strategy whose
entire output is a Decline is a diagnosis with a Strategy's overheads.

**The evidence removed the case it was for.** Every Compose file on the machine this plugin was
designed against — twenty of them, across seventeen repositories — was put through
`docker compose config` as the Compose Strategy runs it. Nineteen render. The one that does not
fails as `service "backend" depends on undefined service "postgres": invalid compose project`,
because its database is declared in `docker-compose.dev.yml` — a filename Compose does not load
without `-f`. A tolerant parse of the canonical file finds no database there either: the database is
in a file nobody loaded.

The fixture the Strategy was written around does not fail at all. `${VAR}` with no default renders
exit 0 with the published port simply absent, not an error; and the render already runs in the
Stack's own directory, so an environment file beside the Compose file is loaded rather than missed.
The service still qualifies on its image, its `5432` target and its `POSTGRES_*` keys, and the
Compose Strategy resolves it today.

## Considered options

**Widen the filenames a Stack renders** — retry with sibling Compose files when the canonical render
fails. The only option that resolves the one real failure. Rejected: of the four Stack directories on
this machine holding non-canonical siblings, two hold `compose.prod.yaml` and `compose.realstack.yaml`.
A rule that loads what Compose declined to load resolves a *production* database for a Project whose
local one is broken — the confidently-wrong Pane this plugin exists to prevent — and no rule
separating `.dev.` from `.prod.` is anything but a guess about a filename.

**Parse the Compose file directly when the render fails.** Rejected on both grounds above: it can
return nothing ADR-0008 permits into a DSN, and across twenty real Stacks there is no fixture where
it would answer.

## Consequences

A Stack that will not render becomes a first-class Decline — naming the Stack's file and quoting
Compose's own refusal — rather than the silence it is today, where a Project whose only Stack is
unrenderable is indistinguishable from one holding no Compose file at all.

Because that text is Compose's own and Compose echoes author-controlled values into it — the message
in `${VAR:?message}` is written to stderr verbatim, and an invalid interpolated value is echoed with
it — it is redacted where it is captured, so an unredacted string never reaches a Diagnosis.
`tests/redaction.rs` scans source for DSN literals and cannot reach runtime text.

Reopening this needs new evidence, and specifically one thing: a Compose file that fails to render
whose database is discoverable *in the file that failed*. The survey behind this is a snapshot of one
machine, so a single fixture would do it.
