---
id: 8
title: Restore the terminal when the app panics
type: bug
status: planned
created: 2026-09-11
updated: 2026-09-11
priority: p0
effort: s
area: ui
release: v0.1
---

## What happens

`leave()` runs only when `run()` returns normally. A panic unwinds straight past it, leaving raw mode on and the alternate screen active — the user's shell is unusable and needs a blind `reset`.

## What should happen

A panic hook installed before `enter()` should restore the terminal and then print the panic normally.

## Reproduction

1. Add a `panic!()` anywhere in the event loop.
2. Run `rsst` and trigger it.
3. The shell is left in raw mode with no echo.
