---
id: 71
title: Middle click does not open anything
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
release: v1.6
effort: s
area: ui
---

## Problem

Middle click does nothing. It is the conventional way to open a link without leaving the page, and rsst already knows how to open both an entry and a numbered link in an article — the gesture is simply not wired to anything.

## Proposal

Middle click opens whatever is under the pointer: an entry row, or a link in the article.

## Acceptance criteria

- [ ] Middle click on an entry opens it in the browser
- [ ] Middle click on a link in the article opens that link
- [ ] Middle click on anything else does nothing
