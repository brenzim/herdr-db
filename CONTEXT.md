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

### The surface

**Client**:
The third-party terminal database browser the plugin launches and does not reimplement.
The plugin's contribution ends at handing it a DSN.
_Avoid_: UI, viewer, browser, frontend

**Pane**:
The herdr surface that hosts the Client as a real terminal process with full keyboard
input. The reason a terminal client can be the plugin's entire user interface.
_Avoid_: window, split, tab, view
