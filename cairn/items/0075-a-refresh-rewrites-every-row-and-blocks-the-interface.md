---
id: 75
title: A refresh rewrites every row and blocks the interface
type: bug
status: backlog
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

- [ ] A refresh that adds one entry to a large feed writes about one entry
- [ ] An unchanged refresh writes nothing
- [ ] Entries that left the feed are still removed, and starred ones still kept
- [ ] Ordering survives, including for kept entries no longer published
- [ ] The interface stays responsive while a large feed is stored
- [ ] A benchmark covers the case, so it cannot quietly return
