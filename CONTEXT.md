# herdr-db

A herdr plugin that opens a PostgreSQL browser in a herdr pane, already connected to the
database belonging to the work the user is currently doing. Its subject matter is not
databases — it is *connection resolution*: deciding, from nothing but the place herdr was
invoked from, which PostgreSQL server and database the user meant.

## Language

### The thing being resolved

**Project**:
The unit of code that owns a database — in practice a repository, identified by its root.
Resolution is keyed on the Project, so every Worktree of a repository resolves alike.
_Avoid_: repo, workspace, package

**Worktree**:
The specific checkout herdr was invoked from. It identifies the Project but does not, by
itself, distinguish one database from another.
_Avoid_: checkout, working copy, branch

**Invocation Context**:
What herdr states about where it was invoked from when it opens the Pane — the Worktree's
repository root, the focused Pane's cwd, the workspace's. The only statement about the
user's location the plugin trusts; the process working directory is never one (ADR-0004).
_Avoid_: environment, launch context, cwd

**DSN**:
The `postgres://…` address that the Client is launched against. The single output of
resolution, and the only thing the Client is told.
_Avoid_: connection string, connection URI, URL

### Resolution

**Connection Resolution**:
Turning a Worktree into a DSN. The plugin's entire subject matter.
_Avoid_: connection discovery, lookup, autodetection

**Resolution Strategy**:
One self-contained attempt to derive a DSN from a Project, which either produces a
Candidate or declines. Strategies are ordered, and the order is the plugin's opinion about
what is most likely to be correct.
_Avoid_: resolver, provider, driver, backend

**Candidate**:
A DSN a Strategy proposes, together with where it came from. More than one may exist for a
single Project, which is what makes resolution a judgement and not a lookup.
_Avoid_: match, result, hit

**Override**:
A user-authored statement that pins a Project's DSN directly, bypassing the Strategy
ordering. The escape hatch for everything the Strategies cannot know.
_Avoid_: config, setting, manual connection

**Plan**:
The whole outcome of Connection Resolution for one invocation: a Launch or a Decline, and
nothing else. The single seam the plugin is tested at — what a Plan says is the behaviour,
how it was arrived at is not.
_Avoid_: decision, outcome, result, action

**Launch**:
A Plan that starts the Client: the argv, the title, and whether it opens read-only. The
only route a DSN takes to the Client.
_Avoid_: command, spawn, run, exec

**Decline**:
A Plan that resolves nothing and carries a Diagnosis instead. A first-class outcome rather
than an error — the Pane stays open showing it.
_Avoid_: error, failure, abort, exit

**Diagnosis**:
Why a Decline declined, in the user's terms. One per distinct fault, because each fault has
a different remedy; it is what the Pane draws.
_Avoid_: error message, reason, cause

### The surface

**Client**:
The third-party terminal database browser the plugin launches and does not reimplement.
The plugin's contribution ends at handing it a DSN.
_Avoid_: UI, viewer, browser, frontend

**Pane**:
The herdr surface that hosts the Client as a real terminal process with full keyboard
input. The reason a terminal client can be the plugin's entire user interface.
_Avoid_: window, split, tab, view

**Host**:
Everything outside the process that Connection Resolution may consult — a file's contents,
a directory's entries, a command's output — injected as a port so a Strategy can be driven
from a test with neither Docker nor a live PostgreSQL server. Every method answers with an
absence rather than an error.
_Avoid_: system, environment, filesystem, io
