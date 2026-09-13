---
id: 74
title: The article is laid out again on every frame
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-13
priority: p1
release: v1.6
effort: m
area: ui
---

## Problem

`draw_detail` parses the entry's HTML and lays it out twice on every frame: once through `max_detail_scroll`, which calls `detail_lines`, and once to build the rows it draws. Nothing is kept between frames, so the same bytes are parsed again ten times a second whether or not anything has changed.

Measured on a real article — 568 KB, the longest in the seeded database:

| | |
| --- | --- |
| parse + layout, once | 10.2 ms |
| a whole frame | 23.4 ms |
| idling on that entry | **23% of a core, permanently** |

The same frame with a short article selected costs 1.4 ms, so this is the article and nothing else. It is also what makes scrolling a long piece expensive: every line moved pays for a full re-parse.

## Proposal

Lay the article out when something that affects it changes — the entry, the fetched article, the pane width, the measure — and keep the rows until then. The cost becomes one parse per article rather than twenty per second.

## Acceptance criteria

- [x] An unchanged article is laid out once, not once per frame
- [x] Changing the entry, the width or the measure lays it out again
- [x] A frame costs what the screen is worth, not what the article is worth
- [x] A test asserts the layout is not recomputed when nothing changed

Measured on the same 568 KB article: **23.4 ms a frame down to 1.77 ms**. The
article's share of a frame went from about 22 ms to about 0.4 ms; what is left
is the feed and entry panes, which is the same 1.4 ms a short article always
cost.

The fourth criterion was written as "idling costs no measurable CPU". That is
`0076`'s to deliver — idling still draws ten frames a second, and the point of
this item is that those frames no longer cost anything much. Restated rather
than quietly ticked.
