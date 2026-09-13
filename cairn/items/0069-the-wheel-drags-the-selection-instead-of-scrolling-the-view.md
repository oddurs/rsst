---
id: 69
title: The wheel drags the selection instead of scrolling the view
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-13
priority: p0
release: v1.6
effort: m
area: ui
---

## Problem

The wheel does not scroll the entry list. It drags the *selection* through it: `Hit::Scroll` calls `select_next` once per line, which is the same thing `j` does. Three consequences, all of them wrong, and all of them reported as "when I scroll down it sometimes just keeps scrolling":

- **It never reaches an end.** `app::step` wraps by design, which is right for `j` and wrong for a wheel. Scrolling down past the last entry returns to the top and keeps going, so a trackpad flick has nothing to stop against.
- **It marks everything it passes as read.** `select_next` calls `mark_current_read`. Scrolling through a list to look at it therefore consumes it.
- **It flickers the article.** Every line resets `detail_scroll` and swaps the detail pane to whatever is passing under the cursor.

Every other application scrolls the viewport and leaves the cursor alone.

## Proposal

Give the feed and entry panes an explicit scroll offset that the wheel moves, and that stops at both ends. The selection stays where it is; the keyboard still scrolls the view to follow it when it moves out of sight.

## Acceptance criteria

- [x] The wheel scrolls the view and does not move the selection
- [x] Scrolling stops at the top and bottom rather than wrapping
- [x] Scrolling marks nothing read and does not disturb the article
- [x] Moving the selection off-screen with the keyboard scrolls it back into view
- [x] Clicking a row still selects the row actually drawn there, at any offset

## 2026-09-13

Verified in the real binary: a 120-event flick redraws once and then emits nothing for the next 4.5 seconds. Before, the wrap meant the list cycled and a flick had nothing to stop against.
