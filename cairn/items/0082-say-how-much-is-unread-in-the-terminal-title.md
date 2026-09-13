---
id: 82
title: Say how much is unread in the terminal title
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
release: v1.7
effort: s
area: ui
---

## Problem

Nothing outside the window says anything. A terminal tab running rsst is called whatever the shell called it, and a reader with the window on another desktop has no way to know that forty things arrived.

Setting the title is one escape sequence, it is what every terminal application with a count does, and it costs nothing.

## Proposal

Set the terminal's title to the unread count and restore it on exit, alongside the alternate screen and raw mode that `main` already restores on every path.

Off by a config key for anyone whose terminal or multiplexer does something else with the title.

## Acceptance criteria

- [ ] The title carries the unread count and updates as it changes
- [ ] It is restored on exit, including after a panic
- [ ] A config key turns it off
- [ ] Nothing is written when the terminal does not claim to support it
