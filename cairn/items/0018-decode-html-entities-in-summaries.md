---
id: 18
title: Decode HTML entities in summaries
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p2
effort: s
area: ui
release: v0.2
---

## Problem

`strip_html` removes tags but leaves entities untouched, so summaries display literal `&amp;`, `&#8217;` and `&nbsp;` instead of the characters they stand for.

## Proposal

Decode named and numeric entities after stripping tags, and normalise non-breaking spaces to ordinary ones.

## Acceptance criteria

- [x] `&amp;`, `&#8217;` and `&nbsp;` render as `&`, `'` and a space.
- [x] An unrecognised entity is left as-is rather than dropped.
- [x] Covered by a unit test in `src/feed.rs`.
