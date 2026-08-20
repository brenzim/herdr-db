# A declaration supplies an identity, never a port

The Compose Strategy renders a Stack with Compose's own configuration renderer, but never puts a
rendered port in a DSN. The render says which service is a database and which running container
belongs to the Stack; the container itself supplies the port, the role, the password and the
database name. A service with no running container behind it is never a Launch; where the
Stack declares it as a database itself and the Project resolves nothing at all, it is a
Decline that names it.

## Why

A rendered port is a statement of intent, and it is only ever consulted in the one situation
where a better number is already in hand. If no container is running, the port connects to
nothing and a Launch against it is the confidently-wrong Pane this plugin exists to prevent
([ADR-0007](./0007-pick-when-ambiguous-announce-always.md) makes the title load-bearing precisely
so that "am I in the right database?" always has an answer). If a container *is* running, its
published port is what the DSN can actually reach, and the declaration can only disagree with it
by being stale.

That disagreement is not hypothetical. A Compose service built from a local `Dockerfile` reports
`Config.Image` as `<project>-<service>`, so the Live Docker Strategy's image filter misses it
entirely and the Compose Strategy is the only one that can resolve it. Sourcing that Candidate's
port from the render would mean the one case Compose exists to rescue is also the one case
answered from declaration rather than reality.

## Considered options

**Compose resolves the connection outright**, which is how the originating issue was written.
Rejected: it either launches against a port nothing is listening on, or it launches against a
declared port while holding the container that publishes a different one.

**Compose feeds the Live Docker Strategy** — the render authorises a container the image filter
rejected, and Docker builds the Candidate. Rejected: one Strategy reaching into another's output
breaks the independent, ordered chain, and there is then no Compose-origin Candidate for the
title to name.

## Consequences

Compose's merge, `!override` and `${VAR:-default}` semantics — the whole reason for deferring to
the renderer rather than parsing YAML — matter for *identifying* the database, not for connecting
to it. The render's `environment` block, which resolves `env_file:`, is an identification signal
only and never a source of credentials; a future change that reads a password out of it is
reintroducing declaration as a source of truth.

Because both Strategies can now produce a Candidate from the same running container, Candidates
are deduplicated on container id with the higher-ranked origin surviving. This is what makes
"reality beats declaration" observable: where a compose file and a running container disagree,
one Candidate survives and the title says `docker`.
