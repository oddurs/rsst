---
id: 19
title: Show per-feed fetch status and errors
type: feature
status: backlog
milestone: v0.2
depends_on:
- 15
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: m
area: ui
---

## Problem

A failed feed becomes a fake entry titled 'Failed to load'. It gets the point across but pollutes the entry list, and a feed that is merely slow is indistinguishable from one that is broken.

## Proposal

Track a real per-feed state — idle, fetching, ok, error — and show it as a marker in the feed list, with the error text in the status bar on selection. Drop the placeholder-entry hack.

## Acceptance criteria

- [ ] A broken feed is marked in the feed list without inventing entries.
- [ ] Selecting it shows the underlying error.
- [ ] A feed that is fetching is visibly distinct from one that failed.
