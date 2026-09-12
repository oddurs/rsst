---
id: 23
title: Star entries and keep them
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p2
effort: m
area: storage
release: v0.3
---

## Problem

Entries disappear as they age out of the feed. Anything worth returning to has to be saved somewhere else.

## Proposal

`s` stars the selected entry. Starred entries are exempt from cache pruning and reachable through a saved view.

## Acceptance criteria

- [x] A starred entry survives falling out of the upstream feed.
- [x] Starred entries are listed in their own view.
- [x] Starring persists across restarts.
