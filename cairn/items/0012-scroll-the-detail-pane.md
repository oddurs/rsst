---
id: 12
title: Scroll the detail pane
type: feature
status: planned
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: s
area: ui
release: v0.1
---

## Problem

Entry text is clipped at the bottom of the pane with no way to see the rest. Any full-content feed is unreadable past roughly ten lines.

## Proposal

Track a scroll offset for the detail pane and bind it when the pane has focus. Make the detail pane focusable as a third stop in the Tab cycle.

## Acceptance criteria

- [ ] Long entries scroll with j/k and the arrow keys.
- [ ] The offset resets when a different entry is selected.
- [ ] Scrolling stops at the last line rather than running off into blank space.
