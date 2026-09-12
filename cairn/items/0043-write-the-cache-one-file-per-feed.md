---
id: 43
title: Write the cache one file per feed
type: feature
status: dropped
created: 2026-09-12
updated: 2026-09-12
priority: p2
effort: m
area: storage
release: v1.1
---

## Problem

The whole cache is rewritten on every refresh, not just on exit, and not just for the feed that changed. Refreshing one feed rewrites all of them.

The cost is linear in *total* entries. The benchmark measures it: **50 feeds × 100 entries — 5,000 entries — round-trips in about 14ms**. That is a non-issue at that size, and it is the shape of the problem rather than the size of it. At roughly ten times the entries it is ~140ms on every refresh, and launch parses the lot before drawing.

Read state has the same shape and a worse ending: `read.toml` only ever grows — three candidate keys per entry read — and nothing prunes it.

## Proposal

Keep one cache file per feed, named by a hash of the URL, so a refresh writes only what changed. Keep the atomic temp-and-rename per file, which is what makes a crash mid-write safe.

This is the cheap half of the problem. It removes the rewrite-everything cost without adding a dependency, and it does nothing for search, which stays a linear scan. If the backlog ever grows past what that can carry, [[0044]] is the larger answer.

`docs/stability.md` commits the cache to a versioned migration path, so this needs a real migration from the single file rather than a flag day.

## Acceptance criteria

- [ ] Refreshing one feed writes only that feed's file
- [ ] An existing single-file cache is migrated forward, not discarded
- [ ] Removing a feed from the config removes its file
- [ ] A corrupt or truncated file loses one feed, not the cache
- [ ] `read.toml` is pruned of keys belonging to entries no longer cached and not starred
- [ ] The benchmark shows refresh cost no longer scaling with total entries

## 2026-09-12

Dropped in favour of [[0044]], which replaces the cache file layout entirely — building per-feed TOML files and then deleting them a day later is churn, not progress. If SQLite turns out not to be viable (the cross-compilation risk on 0044 is what decides that) this is the fallback and should be reopened.
