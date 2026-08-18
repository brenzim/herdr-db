# Resolve connections from the Project, not the Worktree

Connection Resolution is keyed on the repository root, so every Worktree of a repository
resolves to the same DSN. The alternatives were a database per Worktree on a shared server
(deriving a database name from the branch) and a Postgres server per Worktree (its own
published port); both were rejected because the dominant setup binds a *fixed* host port in
`docker-compose`, which makes a server per Worktree impossible without manual port juggling.

## Consequences

Two panes opened from two Worktrees of the same repository show the same data — the Worktree
is how we find the Project, not how we tell databases apart. This is a deliberate no, and the
counter-evidence to watch for is a Project that genuinely differentiates its database per
branch; the Override exists so that case is expressible without reopening this decision.
