---
id: 22
title: Mark all read, and mark an entry unread again
type: feature
status: done
assignee: Oddur Sigurdsson
depends_on:
- 20
created: 2026-09-11
updated: 2026-09-12
priority: p1
effort: s
area: storage
release: v0.3
---

## Problem

Read state can only be set one entry at a time, by selecting it. Declaring bankruptcy on a 500-entry feed is impractical, and an entry read by accident cannot be restored.

## Proposal

`A` marks every entry in the current feed read (with confirmation), `M` toggles read state on the selected entry, and `Shift-A` marks everything across all feeds.

## Acceptance criteria

- [x] Marking all read updates counts immediately and survives a restart.
- [x] An entry can be toggled back to unread.
- [x] Bulk marking asks for confirmation first.
