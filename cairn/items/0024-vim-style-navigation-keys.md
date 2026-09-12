---
id: 24
title: Vim-style navigation keys
type: feature
status: backlog
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: s
area: ui
release: v0.3
---

## Problem

Only j/k and the arrows work. Moving through a long list is one keystroke per row.

## Proposal

`g`/`G` for first and last, `Ctrl-d`/`Ctrl-u` for half-page, `n`/`p` for next and previous unread across feed boundaries.

## Acceptance criteria

- [ ] All of the above work in both list panes.
- [ ] `n` crosses into the next feed when the current one is exhausted.
- [ ] Half-page movement respects the pane height.
