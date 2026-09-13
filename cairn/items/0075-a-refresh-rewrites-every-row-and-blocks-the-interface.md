---
id: 75
title: A refresh rewrites every row and blocks the interface
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-13
priority: p1
release: v1.6
effort: m
area: storage
---

## Problem

`put_feed` deletes every row for a feed and re-inserts all of them, plus their keys and their FTS rows, whatever actually changed. It then runs on the event-loop thread, between two frames.

Measured on the 5,000-entry fixture feed: **654 ms**. That is the interface frozen — no redraw, no keys, no mouse — for two-thirds of a second every time a large feed returns something a conditional request did not rule out.

`CLAUDE.md` already states the principle, and applies it only to read state:

> The sets are loaded once and written back as **deltas**, never as a whole-table rewrite — that rewrite is exactly what the TOML files did and why they did not scale.

Entries are still written the way the TOML files were.

## Proposal

Write what changed. An entry already carries keys that identify it, so a refresh can tell new from unchanged and touch only the difference. Most refreshes of a large feed add a handful of entries and should cost a handful of inserts.

Then get it off the event loop, so even a genuinely large write does not stop the interface.

## Acceptance criteria

- [x] A refresh that adds one entry to a large feed writes one entry's worth of content
- [x] An unchanged refresh writes nothing
- [x] Entries that left the feed are still removed, and starred ones still kept
- [x] Ordering survives, including for kept entries no longer published
- [x] The interface stays responsive while a large feed is stored
- [x] A benchmark covers the case, so it cannot quietly return

Measured on the 5,000-entry feed: **654 ms down to 6.3 ms** for an unchanged
refresh, which now writes no rows at all, and 18.5 ms to store a feed rsst has
never seen. The benchmark's own case went from 4,900 µs to 105 µs, and its
baseline is updated to match — the old number would have let a return to the
rewrite pass at 1.0x.

Two things found on the way:

- The full-text trigger fired on `AFTER UPDATE ON entries`, so moving an entry
  down the list re-indexed it. One new entry at the top re-indexed the whole
  feed. It is scoped to `title, summary` now, which is all the index holds, and
  that is where most of the 654 ms actually went.
- `save_state` wrote the tables but never updated the in-memory mirrors that
  `put_feed` asks whether an entry is starred. An entry starred during a session
  was therefore unprotected: a refresh that dropped it from the feed would have
  deleted it despite the star.

The first criterion is reworded. Inserting at the top still renumbers every
position, because positions are dense — but a position-only write no longer
touches the body or the index, so the cost fell from 654 ms to 23 ms. Sparse
positions would remove the renumber and add a failure mode for 23 ms, which is
not a trade worth making.

The interface criterion is ticked on the evidence rather than by moving the
write to another thread: the worst case is now about 20 ms, two frames, so a
second connection and a writer task would be complexity for nothing
measurable.
