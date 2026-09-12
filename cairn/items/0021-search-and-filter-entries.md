---
id: 21
title: Search and filter entries
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p1
effort: m
area: ui
release: v0.3
---

## Problem

Finding an article you remember reading means scrolling every feed by hand.

## Proposal

`/` opens an incremental search over titles and summaries across all feeds, with `n`/`N` to step through matches and Esc to dismiss.

## Acceptance criteria

- [x] Search matches across every feed, not just the selected one.
- [x] Results update as the query is typed.
- [x] Esc restores the previous selection.
- [x] Search is case-insensitive.
