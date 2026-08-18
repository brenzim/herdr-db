# Pick when ambiguous, announce always

**Status:** accepted, supersedes [ADR-0006](./0006-announce-the-chosen-candidate-rather-than-confirm-it.md)

When a Project yields two or more Candidates, the user chooses between them in a picker before
the Client launches. When it yields exactly one, nothing is shown and the Client opens
immediately. In both cases the Pane's title states which Candidate is in use and where it came
from.

## What changed since ADR-0006

ADR-0006 declined the picker on the grounds that it cost a keystroke on every open and that
chooser UI was the sort of thing [ADR-0001](./0001-wrap-a-terminal-client-in-a-pane.md) ruled
out. Both grounds turned out to be weaker than they looked. herdr supports `placement = "popup"`
with sized dimensions, so the picker is a manifest entry rather than a UI framework — and
ADR-0001's "no database UI" means no grid, no query editor, no result rendering, which a
Candidate chooser is not. The keystroke cost also only applies when there is something to
choose: roughly fifteen of the seventeen surveyed Postgres projects yield a single Candidate and
never see the picker.

## Consequences

The picker's *appearance is itself the signal* that a Project is ambiguous — deliberately
inconsistent, because the inconsistency carries information that a dialog shown every time
would not. The title remains load-bearing and is not replaced by the picker: a single-Candidate
open shows no picker at all, so the title stays the only on-screen statement of what was
connected to, both at launch and a week later. Because displaying several Candidates is now
cheap, an Override may declare more than one named connection for a Project; they enter the
picker alongside the Strategies' Candidates.
