---
id: 70
title: A click passes through the prompt covering it
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-13
priority: p1
release: v1.6
effort: s
area: ui
---

## Problem

`Hits` knows the help overlay is covering the screen and gives it every click. It does not know about the other two overlays: the "add a feed" prompt and the "move to folder" picker. Both are drawn over everything, and a click on either lands on the pane behind it — selecting a feed, or opening an entry, underneath a prompt that is still waiting for typing.

## Proposal

One flag for "an overlay owns the screen", set for all three. A click that lands on a prompt should do nothing rather than something invisible.

## Acceptance criteria

- [x] A click while the add prompt is open does not reach the panes
- [x] The same for the move picker
- [x] A test asserts it for each overlay, so a fourth one cannot quietly regress it
