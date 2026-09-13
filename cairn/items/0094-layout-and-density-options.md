---
id: 94
title: Layout and density options
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p3
release: v1.9
effort: m
area: ui
---

## Problem

The layout is one arrangement: a sidebar, an entry list above an article, at fixed proportions. It suits a wide window and is cramped on a narrow one, where the article gets a third of a screen that was already short.

There is also one list density — every entry takes one row with a date on the right — and no say over the date format.

## Proposal

A few honest choices rather than a layout engine: where the article sits relative to the list, how much of the width the sidebar takes, whether the sidebar is shown at all, and a date format. `0055` already showed the value of getting the lists out of the way.

## Acceptance criteria

- [ ] The article can sit beside the entry list as well as below it
- [ ] The sidebar can be hidden, and its width set
- [ ] The date format is configurable, with a sensible default
- [ ] Every layout works at 80x24 and at 200x60
- [ ] ASCII mode still draws nothing a plain terminal cannot
