---
id: 15
title: Cache feeds on disk so launch is instant and offline works
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p0
effort: l
area: storage
release: v0.2
---

## Problem

Everything lives in memory. Launch always blocks on the network, and with no connection the reader is empty — exactly when a backlog of already-downloaded articles would be most useful.

## Proposal

Persist parsed entries under the cache directory, keyed by feed URL. Render from cache on launch and refresh in the background. Prune entries that have fallen out of the feed and are already read.

## Acceptance criteria

- [x] A second launch renders before any network request completes.
- [x] With the network down, previously fetched entries are readable.
- [x] Cache growth is bounded — old read entries are pruned.
- [x] A corrupt or truncated cache file is discarded rather than crashing the app.
