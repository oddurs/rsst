---
id: 72
title: A feed's marker and its count are in different columns
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-13
priority: p2
release: v1.6
effort: s
area: ui
---

## Problem

A feed's unread count is right-aligned one column further right than its status marker, so the `!` on a failed feed sits a column left of where the numbers are and the right-hand edge of the sidebar is ragged.

Measured at 120 columns: the count ends at column 35, the `!` at column 34.

```
│ │ │   Awkward Publishing      3 │
│ │ │   Truncated              !  │
```

## Proposal

One column for whichever of the two a row has, so the edge is straight whatever a feed is doing.

## Acceptance criteria

- [x] A marker and a count occupy the same column
- [x] A test asserts it rather than a person noticing
