---
id: 60
title: ship leaves a half-shipped branch when a label is missing
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p2
release: v1.4
effort: s
area: ci
---

## Problem

`scripts/ship finish` passes the item's `area` to `gh pr create` as `--label area:<area>`. GitHub refuses a label that does not exist, `gh` exits non-zero, and `set -euo pipefail` ends the script — *after* the item has been closed, the roadmap re-rendered, the work committed and the branch pushed, but *before* the pull request exists.

The branch is then in a state `finish` cannot recover: re-running it says `nothing to commit` and stops, so the pull request has to be opened by hand.

Hit for real on `0058`, whose area was `dev` — the first item in a new area, which is exactly when this happens.

## Proposal

Create the label if it is missing, before opening the pull request. A new area is a normal thing to invent and should not need the repository's labels edited first.

Failing that, at minimum fail *before* the commit rather than after it.

## Acceptance criteria

- [x] An item whose area has no label still opens a pull request
- [x] The label is created with the same colour and shape as the others
