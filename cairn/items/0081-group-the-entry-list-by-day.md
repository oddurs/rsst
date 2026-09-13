---
id: 81
title: Group the entry list by day
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
part_of:
- 80
release: v1.7
effort: s
area: ui
---

## Problem

The entry list is a flat run of titles with a date on the right. Thirty entries from this morning and thirty from last March look the same until you read the dates one at a time.

Readers that group by day — "Today", "Yesterday", "Last week" — let someone see at a glance how much is new, which is the question they opened the reader to ask.

## Proposal

Optional day headings in the entry list, in the same style as the folder headings in the sidebar so the interface has one idea of a heading rather than two.

Only sensible when sorting by a date, so it needs `0080` first: grouping by day a list sorted by title would be nonsense, and the setting should say so rather than producing it.

## Acceptance criteria

- [ ] Entries are grouped under day headings when sorted by a date
- [ ] The headings are "Today" and "Yesterday" where that reads better than a date
- [ ] Grouping is off when the sort is not by date, without an error
- [ ] Headings are not selectable and do not break keyboard or wheel navigation
- [ ] The hit map skips them, so a click near one does not select the wrong entry
