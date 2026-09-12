---
id: 68
title: A feed with an empty title shows as nothing
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p2
release: v1.5
effort: s
area: net
---

## Problem

`danluu.com/atom.xml` publishes `<title></title>`. The fallback to the URL only fires when the element is absent, so an empty one wins and the feed sits in the sidebar as a blank line with a number beside it.

## Proposal

Treat blank as absent, for the feed title and the entry title alike. An entry already falls back to `(untitled)`; the same reasoning applies to whitespace.

## Acceptance criteria

- [x] A feed whose title is empty or whitespace falls back to its URL
- [x] An entry whose title is empty falls back to `(untitled)`
- [x] A test uses the shape danluu.com actually publishes
