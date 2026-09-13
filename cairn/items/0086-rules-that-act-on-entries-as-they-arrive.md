---
id: 86
title: Rules that act on entries as they arrive
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
part_of:
- 84
---

## Problem

Everything that arrives is treated identically. A reader following a high-volume feed for the one post in fifty they care about has to skim fifty, and a reader who never wants a particular author's posts has to skip them forever.

newsboat has had ignore rules for twenty years, and they are the reason people with two hundred feeds can use it.

## Proposal

Rules applied to entries as they arrive, each a condition and an action:

- **star** it
- **mark read** — the polite kill-file: still there, not in the way
- **ignore** — never stored at all, for the genuinely unwanted
- **tag** it, once `0087` exists

Conditions should reuse `0084`'s query language rather than inventing a second one; a rule is a saved query with something to do.

Ignoring needs care: an entry not stored cannot be un-ignored by changing the rule, so the reader has to be able to see what a rule is catching before it is applied for real.

## Acceptance criteria

- [ ] Rules live in the config, with a condition and an action
- [ ] They run on arrival, before the entry reaches the list
- [ ] A rule can be tried against what is already stored, showing what it would catch, without acting
- [ ] Rules never touch an entry the reader has already read or starred
- [ ] The order rules run in is defined and written down
