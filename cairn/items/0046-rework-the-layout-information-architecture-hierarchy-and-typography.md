---
id: 46
title: 'Rework the layout: information architecture, hierarchy and typography'
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

Rendered with realistic content, the interface has a set of related faults. They are all the same mistake in different places: the screen gives prominence by position and weight without regard to what is worth reading.

**Information architecture**

- **The largest pane is titled with a URL.** `http://127.0.0.1:8932/rust.xml` is the least useful string available, and it sits in the most prominent label on screen. The feed's name is right there and unused.
- **`Detail` and `Feeds` are labels for boxes, not information.** They spend the border — scarce, high-attention space — restating what is obvious from looking.
- **Nothing reports state.** No total unread, no indication of which view you are in. The status bar is entirely key hints.
- **The article's URL is the second thing you read**, given equal weight to the title and placed above the body. It is reference data, not content. Meanwhile the entry's date, which *is* useful context, is absent from the detail pane and shouted in the list.

**Hierarchy**

- **The date leads every entry row**, ten characters of low-value text pushing every title to column 16 and forming a wall down the left edge. Titles are the content; they should lead.
- **Unread counts trail the feed name** with two spaces, so they never align into a column and read as part of the title.
- **Grouped and ungrouped feeds sit at different indents** — a grouped feed at 2, an ungrouped one at 0 — so an ungrouped feed looks like a sibling of the group headings rather than a peer of the feeds.
- **The selection marker shifts the text it marks**, so moving the cursor moves the words.

**Typography**

- No padding inside any pane: text touches the border on both sides.
- Two adjacent vertical borders between panes — a two-column band of pure chrome down the full height.
- Body text runs to the full pane width with no measure control.
- Panes are a fixed 60/40 regardless of content, so a three-entry feed leaves most of the screen empty while the article is cramped.

## Proposal

Give position and weight to what is worth reading:

- pane titles carry information: the feed's name, what is unread, which view is active
- entry rows lead with the title; the date goes right, short, dimmed
- feed rows align at one indent with counts right-aligned in a gutter
- the selection marker lives in a fixed gutter so text never moves
- one column of padding inside panes, one column of gap between them
- the entry list sizes to its content, giving the remainder to the article

## Acceptance criteria

- [x] No pane is titled with a URL; titles say what is in them and what state it is in
- [x] Entry rows lead with the title, with the date right-aligned and dimmed
- [x] Unread counts right-align into a column, and every feed sits at the same indent
- [x] Moving the selection does not shift the text of any row
- [x] Panes have interior padding and are separated by a single column, not two borders
- [x] The entry list sizes to its content, within bounds, and the article gets the rest
- [x] Everything still fits and stays legible at 80x24, and in ASCII mode
- [x] Mouse hit-testing still lands on the right row after the layout changes
