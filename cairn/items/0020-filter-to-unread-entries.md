---
id: 20
title: Filter to unread entries
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p1
effort: s
area: ui
release: v0.3
---

## Problem

Every entry is always listed, so yesterday's read items sit between today's new ones and the backlog never visibly shrinks.

## Proposal

A toggle (`u`) switching the entry list between all and unread-only, remembered across restarts.

## Acceptance criteria

- [x] `u` toggles unread-only and the state persists.
- [x] Feed unread counts agree with what the filtered list shows.
- [x] Reading the last unread entry leaves a clear empty state rather than a blank pane.
