---
id: 20
title: Filter to unread entries
type: feature
status: backlog
milestone: v0.3
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: s
area: ui
---

## Problem

Every entry is always listed, so yesterday's read items sit between today's new ones and the backlog never visibly shrinks.

## Proposal

A toggle (`u`) switching the entry list between all and unread-only, remembered across restarts.

## Acceptance criteria

- [ ] `u` toggles unread-only and the state persists.
- [ ] Feed unread counts agree with what the filtered list shows.
- [ ] Reading the last unread entry leaves a clear empty state rather than a blank pane.
