---
id: 9
title: Persist read and unread state
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-11
priority: p0
effort: m
area: storage
release: v0.1
---

## Problem

Every entry looks unread on every launch. With no memory of what has been seen, the reader cannot answer the only question that matters — what is new since last time.

## Proposal

A small state file keyed by entry id (falling back to link, then title+date hash) recording what has been read. Mark an entry read when it is selected; persist on exit and after each change. Show unread counts per feed and style unread entries distinctly.

## Acceptance criteria

- [x] Reading an entry and restarting leaves it marked read.
- [x] Each feed shows an unread count.
- [x] A feed whose entry ids change between fetches does not resurrect read entries.
- [x] The state file is written atomically so a crash mid-write cannot corrupt it.
