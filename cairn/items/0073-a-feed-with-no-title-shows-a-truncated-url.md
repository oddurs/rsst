---
id: 73
title: A feed with no title shows a truncated URL
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

`danluu.com` publishes `<title></title>`, so `0068` falls back to the URL — and the sidebar now reads `https://danluu.com/atom.…`, which is both wider than every other name and truncated to no purpose. The fallback is right; the thing it falls back to is not.

## Proposal

Fall back to the host, which is what a person would call the feed anyway. `danluu.com`, not `https://danluu.com/atom.xml`.

## Acceptance criteria

- [x] A feed with no title is named by its host
- [x] A URL that cannot be read as one still falls back to something
