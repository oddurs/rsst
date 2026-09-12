---
id: 42
title: Make the reader mouse-first
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p0
effort: l
area: ui
release: v0.4
---

## Problem

rsst is keyboard-only. Every pane, list and control assumes a keystroke, so the obvious thing — click the feed you want to read — does nothing at all. A reader is a pointing-shaped task: you look at a list and pick one.

Mouse support in a TUI is also easy to do badly. Capturing the mouse takes text selection away from the terminal, so a careless implementation trades one obvious gesture for another.

## Proposal

Full pointer support, hit-tested against what was actually drawn rather than guessed from geometry:

- click a feed, an entry, or a group heading to select or fold it
- wheel over any pane scrolls that pane, including the article text
- click the link in an article to open it; double-click an entry to do the same
- the key hints along the bottom become real buttons
- click anywhere to dismiss the help overlay
- `mouse = false` in the config for people who want their terminal's selection back, and Shift-drag as the escape hatch meanwhile

## Acceptance criteria

- [x] Clicking a feed, entry or group heading selects or folds exactly the row under the pointer, at any scroll offset
- [x] The wheel scrolls whichever pane the pointer is over, and the detail pane scrolls by more than one line at a time
- [x] Clicking a link opens it; double-clicking an entry opens it
- [x] The status bar hints are clickable and do what they say
- [x] Clicking dismisses the help overlay
- [x] `mouse = false` disables capture entirely, and the trade-off with text selection is documented
- [x] Hit-testing is derived from the drawn layout, so it cannot drift from what is on screen
