---
id: 76
title: The interface redraws when nothing has changed
type: bug
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
release: v1.6
effort: m
area: ui
part_of:
- 74
---

## Problem

The event loop draws a frame, then polls for input with a 100 ms timeout, then draws again — whether or not anything changed. Sitting still with rsst open costs ten full renders a second forever.

It is cheap when the article is short (1.4 ms a frame) and not cheap when it is not (23.4 ms), but the point is that none of it is work anyone asked for. On a laptop it is a background drain with nothing to show for it.

## Proposal

Draw when something has changed: a key, a mouse event, a fetch landing, a timer firing. Otherwise wait.

Best done after `0074`, which removes most of what a wasted frame costs, so the benefit here is measured against the right baseline.

## Acceptance criteria

- [ ] An idle reader draws nothing
- [ ] Every input and every arriving fetch still draws promptly
- [ ] Nothing that used to appear on its own stops appearing
