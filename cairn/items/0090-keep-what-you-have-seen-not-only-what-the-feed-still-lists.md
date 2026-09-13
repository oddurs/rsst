---
id: 90
title: Keep what you have seen, not only what the feed still lists
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

`put_feed` deletes entries the publisher has dropped, unless they are starred. So rsst's memory is exactly as deep as the feed's window — typically the last ten or fifty entries — and search can only reach what the publisher still lists.

That is a defensible choice for a cache and the wrong one for a reader. The value of having read something for two years is being able to find it, and rsst throws it away on the refresh after it scrolls off.

## Proposal

Keep what has been seen. An entry leaving the feed stops being *current* rather than ceasing to exist: it keeps its row, its read state and its position in history, and stops appearing in the feed's list unless asked for.

This wants a column saying when the entry was first seen and whether the feed still lists it, and `0080` wants the first of those anyway.

The reason to do it before `0091` is that a retention policy is meaningless without something to retain.

## Acceptance criteria

- [ ] An entry the publisher drops is kept, not deleted
- [ ] The feed's list shows what the feed currently lists
- [ ] History is reachable through the query language and the index
- [ ] Read and starred state survives an entry leaving and returning
- [ ] The delta write from `0075` stays a delta
- [ ] A benchmark covers a database with far more history than current entries
