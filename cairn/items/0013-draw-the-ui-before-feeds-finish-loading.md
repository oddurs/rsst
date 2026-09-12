---
id: 13
title: Draw the UI before feeds finish loading
type: feature
status: planned
milestone: v0.1
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: m
area: ui
---

## Problem

`fetch_all` runs before `enter()`, so a slow or unreachable feed leaves the user staring at a bare shell for up to 15 seconds with no indication the program is alive.

## Proposal

Enter the TUI immediately with feeds in a loading state, then fill them in as each fetch resolves.

## Acceptance criteria

- [ ] The UI appears within about 100ms regardless of network conditions.
- [ ] Each feed shows a loading indicator until it resolves.
- [ ] Quitting during the initial load exits cleanly and restores the terminal.
