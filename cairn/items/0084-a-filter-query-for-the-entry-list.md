---
id: 84
title: A filter query for the entry list
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

Filtering is three toggles: unread-only, starred, all-feeds. With 3,590 unread in the seeded environment, that is not a way of finding anything — it is a way of looking at everything or looking at a smaller everything.

Search exists and is separate: it throws away the current view, returns matches across all feeds, and cannot be combined with a filter.

## Proposal

One query that filters the entry list, in the form readers already know:

```
is:unread from:lobsters since:7d "borrow checker"
```

Terms to support: `is:unread`, `is:starred`, `from:<feed>`, `in:<folder>`, `since:`/`before:` with both dates and durations, `title:` for the headline alone, and bare words matching the text through the index that is already there.

The existing toggles become shorthands that write into the same query, so there is one idea of "what am I looking at" rather than four that interact.

## Acceptance criteria

- [ ] A query filters the entry list live as it is typed
- [ ] Every listed term works, and terms combine
- [ ] An unparseable query says which part it could not read, and filters nothing
- [ ] The existing toggles set the equivalent query
- [ ] The query is visible while it is in force, and easy to clear
- [ ] Matching runs through the full-text index rather than a scan
