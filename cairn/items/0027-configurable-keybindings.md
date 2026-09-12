---
id: 27
title: Configurable keybindings
type: feature
status: backlog
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: m
area: config
release: v0.4
---

## Problem

Every binding is hardcoded in the match in `main.rs`. Dvorak and Colemak users, and anyone with different muscle memory, are stuck with it.

## Proposal

A `[keys]` table in the config mapping action names to keys, merged over the defaults so a partial override works.

## Acceptance criteria

- [ ] Rebinding an action takes effect on next launch.
- [ ] An unknown action name is reported clearly rather than ignored.
- [ ] The help overlay reflects the active bindings.
