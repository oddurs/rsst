---
id: 48
title: Render articles as documents, not one long paragraph
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p0
effort: l
area: ui
release: v1.2
---

## Problem

The detail pane destroys the structure of everything it shows. `strip_html` deletes every tag and collapses whitespace, so a code block, a list, a quote and a link all arrive as one undifferentiated paragraph.

A real article renders like this today:

```
The borrow checker now accepts this: fn main() { let mut v =
vec![1, 2, 3]; let first = &v[0]; println!("{first}");
v.push(4); } Three things changed: Loans end at last use, not
end of scope Closures capture disjoint fields See RFC 2094
for detail This is the biggest change since NLL.
```

That was the right function when its job was to make a summary fit one line of a list. It is the wrong one now that the detail pane is a reading surface. For a reader whose feeds are technical, a mangled code sample is close to unusable — and reading is the thing the whole program exists for.

## Proposal

Parse the entry's HTML into a small document model — paragraphs, headings, code, lists, quotes, links — and lay that out for a fixed-width pane:

- paragraphs separated, wrapped to the measure
- **code preserved verbatim** and not reflowed, which is the whole point of a code block
- list items bulleted with a hanging indent
- quotes marked down the left
- links numbered in the text with a reference list at the end, the way a terminal browser does

Tolerant parsing: feeds in the wild are full of unclosed tags and invented markup, and none of it should be able to panic the reader.

## Acceptance criteria

- [x] Paragraphs are separated rather than run together
- [x] Code blocks keep their own line breaks and indentation and are not reflowed
- [x] List items are bulleted with a hanging indent, ordered lists numbered
- [x] Block quotes are visually distinct from body text
- [x] Headings are distinct from body text
- [x] Links are numbered in the text with a reference list, so the URL is reachable without leaving
- [x] Malformed and hostile HTML cannot panic or hang the renderer
- [x] The plain-text summary used for the entry list and full-text search still works
