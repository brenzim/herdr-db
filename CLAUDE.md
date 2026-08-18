# herdr-db

A herdr plugin (`id = "db"`) that opens a PostgreSQL browser in a herdr Pane, already
connected to the database belonging to the Project that the Worktree herdr invoked it from
identifies. Resolution is keyed on the Project, so every Worktree of a repository resolves
alike (ADR-0003).

## Agent skills

### Issue tracker

Issues and specs live in this repo's GitHub Issues (`brenzim/herdr-db`), driven via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles, each label string equal to its name (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` at the repo root plus `docs/adr/`. See `docs/agents/domain.md`.
