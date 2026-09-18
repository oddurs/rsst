---
id: 97
title: The key reference no longer fits a forty-row terminal
type: bug
status: backlog
created: 2026-09-17
updated: 2026-09-17
priority: p2
release: v1.7
effort: s
area: ui
---

## Problem

The key reference is drawn in one column: a heading per section, a row per binding. Thirty bindings in five sections need 39 rows plus a border, and a forty-row terminal has 38 to give — so it falls back to a compact layout that fits more by dropping the section headings entirely.

Losing the headings is losing the only structure the list has. It went over the line when `0077` added the three feed actions, and it will go further over with every action added after.

## Proposal

Two columns when the terminal is wide enough. At 100 columns a key column and a description fit twice over with room to spare, which buys back more than the headings cost and postpones the question for a long time.

The compact fallback can stay for genuinely small terminals, where one column of everything is still the best that can be done.

## Acceptance criteria

- [ ] A wide terminal lays the reference out in two columns, with the headings
- [ ] Sections are not split across columns where they fit whole
- [ ] A narrow terminal is unchanged
- [ ] Scrolling still works in every layout
- [ ] `a_roomy_terminal_also_gets_the_section_headings` passes at 100x40 again
