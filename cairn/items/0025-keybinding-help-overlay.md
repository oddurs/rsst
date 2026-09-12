---
id: 25
title: Keybinding help overlay
type: feature
status: backlog
milestone: v0.3
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: s
area: ui
---

## Problem

The status bar lists four keys. Everything else has to be learned from the README.

## Proposal

`?` opens a modal listing every binding, generated from the same table the dispatcher uses so it cannot drift.

## Acceptance criteria

- [ ] `?` opens and any key dismisses.
- [ ] The overlay is generated from the binding table rather than hand-maintained.
- [ ] It stays readable at 80x24.
