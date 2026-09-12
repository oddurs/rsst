---
id: 23
title: Star entries and keep them
type: feature
status: backlog
milestone: v0.3
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: m
area: storage
---

## Problem

Entries disappear as they age out of the feed. Anything worth returning to has to be saved somewhere else.

## Proposal

`s` stars the selected entry. Starred entries are exempt from cache pruning and reachable through a saved view.

## Acceptance criteria

- [ ] A starred entry survives falling out of the upstream feed.
- [ ] Starred entries are listed in their own view.
- [ ] Starring persists across restarts.
