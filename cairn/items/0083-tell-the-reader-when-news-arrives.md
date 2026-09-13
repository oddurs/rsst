---
id: 83
title: Tell the reader when news arrives
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

A background refresh lands in silence. `0042` made rsst refresh on a timer, so entries now arrive while nobody is looking, and the only way to find out is to look.

## Proposal

Two ways of saying so, both off by default, because an unbidden noise is worse than no notification at all:

- the terminal bell, which works everywhere and through ssh
- a desktop notification via the platform's own opener, which does not

Both should say how many arrived and from where, and neither should fire on the first fetch after launch — everything is new then, and that is not news.

## Acceptance criteria

- [ ] An optional bell when a refresh brings something new
- [ ] An optional desktop notification, naming the feeds
- [ ] Neither fires for the first load after launch
- [ ] Neither fires for a refresh that brought nothing
- [ ] Both off unless configured
