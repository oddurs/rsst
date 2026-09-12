---
id: 36
title: Define the semver and MSRV policy
type: chore
status: backlog
milestone: v1.0
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: s
area: docs
---

## Problem

Nothing states what 1.0 promises. Users cannot tell whether the config format or the keybindings are safe to depend on.

## Proposal

Document what is covered by semver — config schema, key bindings, state file format — and the MSRV bump policy.

## Acceptance criteria

- [ ] The policy is written down.
- [ ] The state and cache file formats carry a version field with a migration path.
- [ ] The MSRV bump policy is stated and reflected in CI.
