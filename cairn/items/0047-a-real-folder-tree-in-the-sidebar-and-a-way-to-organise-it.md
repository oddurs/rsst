---
id: 47
title: A real folder tree in the sidebar, and a way to organise it
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p1
effort: l
area: ui
release: v1.1
---

## Problem

The sidebar is flat. A feed carries a list of tags and OPML import already captures nested folders as that list — `["Outer", "Inner"]` — but the sidebar reads only `tags.first()`, so every structure deeper than one level is thrown away on the way to the screen. Two feeds in `Rust > Core` and `Rust > Ecosystem` both render as plain members of `Rust`.

There is also no way to organise anything from inside the reader. Moving a feed means quitting, editing TOML by hand, and starting again — which is the same complaint that made `0030` (reload without restarting) worth doing, one level up.

Folder unread counts do not exist either, so a collapsed folder says nothing about whether it is worth opening.

## Proposal

**The tree**

- Build a real tree from the whole tag path, nested arbitrarily deep
- Draw it with guides so membership is unambiguous at a glance
- A folder shows the unread total of everything beneath it
- Fold and unfold at any level, remembered between runs

**Organising**

- Move the selected feed to another folder from inside the reader, choosing from the folders that exist or naming a new one
- Write the change back to `config.toml` **without destroying the comments in it** — the config is hand-written and commented, and a round-trip through a plain serialiser would flatten it

## Acceptance criteria

- [x] Folders nest arbitrarily deep, built from the whole tag path rather than the first tag
- [x] Tree guides make it unambiguous which folder a feed belongs to
- [x] A folder shows the unread count of everything beneath it, including nested folders
- [x] Folding works at any level, hides all descendants, and is remembered between runs
- [x] A feed can be moved to an existing folder, a new folder, or the top level, from inside the reader
- [x] Moving rewrites `config.toml` with its comments and formatting intact
- [x] Everything still fits at 80x24 and in ASCII mode
