---
id: 85
title: Saved searches that behave like folders
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
part_of:
- 84
---

## Problem

A query is worth keeping. "Rust things I have not read", "anything from these three sites this week" — a reader who builds one wants it tomorrow too, and typing it again is how a good idea becomes an unused one.

## Proposal

Saved queries, shown in the sidebar beside the folders and behaving like them: a name, a count of what matches, selectable. They are a view of the same entries rather than a copy, so read state and stars are simply the entry's own.

Needs `0084`.

## Acceptance criteria

- [ ] A query can be saved with a name and appears in the sidebar
- [ ] It shows a live count, like a folder does
- [ ] Selecting it filters the entry list to its matches
- [ ] It can be renamed and removed
- [ ] Saved queries live in the config, where the reader can edit them by hand
