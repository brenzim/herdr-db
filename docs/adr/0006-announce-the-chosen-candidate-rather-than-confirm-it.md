# Announce the chosen Candidate in the pane title rather than confirm it with a picker

**Status:** superseded by [ADR-0007](./0007-pick-when-ambiguous-announce-always.md)

When a Project yields several Candidates, the highest-ranked Strategy wins silently and the
Pane's title states which Candidate won and where it came from. There is no confirmation prompt.

## Considered Options

A picker shown whenever two or more Candidates exist is more obviously correct, and herdr can
render one (`placement = "popup"`). It was rejected because it costs a keystroke on *every*
open, and because building chooser UI is precisely what [ADR-0001](./0001-wrap-a-terminal-client-in-a-pane.md)
said this plugin would not do. Choosing silently with no feedback was rejected for the opposite
reason: a wrong resolution presents as a perfectly ordinary grid of real rows, with nothing on
screen to contradict it.

## Consequences

The title is load-bearing, not decoration — it is the only thing standing between the user and
an unnoticed wrong database, so it must name the database and its origin, and it must never be
truncated to the point of ambiguity. Ambiguity is known to be common rather than exceptional:
one surveyed repository runs two legitimate local Postgres stacks on different ports, where a
silent pick is wrong roughly half the time. A picker remains available later as an explicit
action without reopening this decision.
