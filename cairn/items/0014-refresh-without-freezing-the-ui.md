---
id: 14
title: Refresh without freezing the UI
type: feature
status: planned
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: m
area: net
release: v0.1
---

## Problem

`r` awaits every fetch inline, so the whole app is unresponsive until the slowest feed returns or times out. Keystrokes queue up and fire afterwards.

## Proposal

Move fetching onto a task that reports back over a channel the event loop drains each tick. Keep showing stale entries while the refresh is in flight.

## Acceptance criteria

- [ ] The UI stays responsive during a refresh and `q` still quits.
- [ ] Entries remain readable while their feed is refreshing.
- [ ] Pressing `r` repeatedly does not spawn overlapping refreshes of the same feed.
