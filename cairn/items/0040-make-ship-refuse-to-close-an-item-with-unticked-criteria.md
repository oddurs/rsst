---
id: 40
title: Make ship refuse to close an item with unticked criteria
type: chore
status: done
created: 2026-09-11
updated: 2026-09-11
priority: p0
effort: s
area: ci
release: v0.1
---

## Problem

`scripts/ship finish` closed cairn 0039 while all five of its acceptance criteria were still unticked. cairn printed `5 of 5 acceptance criteria are unticked` and the script carried on regardless, so the one signal that the work actually matches what was asked for is advisory only. An agent optimising for a green pipeline will close items it has not finished.

The `cairn check` CI job was also not in the required status checks, so the backlog gate could go red without blocking a merge.

## Proposal

Gate on it in `finish`: if the item has unticked acceptance criteria, refuse to close, and say which. Add an explicit escape hatch for the case where a criterion is genuinely not applicable, so the gate is not simply worked around by deleting checkboxes.

## Acceptance criteria

- [x] `ship finish` fails when the item has unticked acceptance criteria, naming them
- [x] `SHIP_ALLOW_UNTICKED=1` overrides it for a deliberate partial close
- [x] `cairn check` is a required status check on `main`
- [x] cairn 0039's own criteria are ticked, since the work is done
- [x] `ship start` tolerates a tree dirtied only by `cairn/items` and `ROADMAP.md`, since filing the item is what precedes starting it
