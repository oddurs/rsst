---
id: 21
title: Search and filter entries
type: feature
status: backlog
milestone: v0.3
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: m
area: ui
---

## Problem

Finding an article you remember reading means scrolling every feed by hand.

## Proposal

`/` opens an incremental search over titles and summaries across all feeds, with `n`/`N` to step through matches and Esc to dismiss.

## Acceptance criteria

- [ ] Search matches across every feed, not just the selected one.
- [ ] Results update as the query is typed.
- [ ] Esc restores the previous selection.
- [ ] Search is case-insensitive.
