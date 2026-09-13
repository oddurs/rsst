---
id: 89
title: Search that can be scoped and refined
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
part_of:
- 84
release: v1.8
effort: m
area: storage
---

## Problem

`Db::search` runs one FTS `MATCH` over every entry and returns up to a limit, ordered by feed and position. There is no way to search one feed, or the last month, or titles only, and no way to narrow a result once it is on screen.

It is also a separate mode from filtering, so a reader cannot search and then say "of these, the unread ones".

## Proposal

Fold search into `0084`: a bare word in the query is a full-text match, and everything else is a filter on the same list. Then "search this feed" is `from:x word`, and narrowing is typing more.

The index needs to carry enough to support it — `entries_fts` holds the title and the summary, so `title:` needs the two to be distinguishable rather than one blob.

## Acceptance criteria

- [ ] Searching and filtering are one thing, not two modes
- [ ] A search can be narrowed without being retyped
- [ ] `title:` matches the headline alone
- [ ] Results say how many matched, not just the first page of them
- [ ] The index still answers in the time the benchmark expects
