---
id: 80
title: Sorting is one boolean where it should be a choice
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-17
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

- [x] Sort by published date, received date, title, and feed name
- [x] Direction is chosen separately from the field
- [x] An entry with no published date sorts predictably rather than randomly
- [x] A feed can override the global sort, and the override is remembered
- [x] The current `t` binding still does the obvious thing
- [x] The entry pane says what it is sorted by, so it is never a mystery

Two things turned up that the item did not know about:

- **A single feed was never sorted at all.** `oldest_first` only reached
  `all_entries`, which is the all-feeds view; `visible_indices` returned the
  publisher's order whatever the setting said. Both go through one comparison
  now.
- **`received` needed the column the item predicted**, added as schema 8. Rows
  that predate it have no first-seen date and fall back to their published one,
  because there is no honest value to invent for them.

`t` still reverses, `T` cycles the field, and `Ctrl-t` gives the selected feed
an order of its own. The old `oldest_first` flag is still written, so a reader
who opens the same database with an older rsst finds their list as they left it.
