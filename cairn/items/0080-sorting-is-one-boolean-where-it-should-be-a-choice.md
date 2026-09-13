---
id: 80
title: Sorting is one boolean where it should be a choice
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p1
release: v1.7
effort: m
area: ui
---

## Problem

Sorting is `oldest_first: bool` in `state.rs`, toggled by `t`. That is the whole of it.

Every desktop reader offers a choice of field and a direction independently: by the date the entry was published, by the date rsst first saw it, by title, by feed. The two are not the same — a feed that backfills its archive publishes old entries today, and "newest first" by published date buries them while "newest first" by received date puts them where a reader expects.

There is also no per-feed override, and a comics feed and a news feed want different answers.

## Proposal

A sort field and a direction, each chosen separately, with the field remembered per feed where the reader sets one and a global default otherwise.

`received` needs a column: entries are stored without a record of when they arrived. That is a schema change, and one worth making anyway — `0090` wants the same information.

## Acceptance criteria

- [ ] Sort by published date, received date, title, and feed name
- [ ] Direction is chosen separately from the field
- [ ] An entry with no published date sorts predictably rather than randomly
- [ ] A feed can override the global sort, and the override is remembered
- [ ] The current `t` binding still does the obvious thing
- [ ] The entry pane says what it is sorted by, so it is never a mystery
