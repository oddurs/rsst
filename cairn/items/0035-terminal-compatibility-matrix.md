---
id: 35
title: Terminal compatibility matrix
type: chore
status: backlog
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: m
area: ui
release: v1.0
---

## Problem

Only tested in one terminal on macOS. Unicode box drawing, colour depth and key reporting all vary, and the Windows key-release behaviour is already a known hazard.

## Proposal

Test across the common terminals on all three platforms, document what works, and add an ASCII fallback where box drawing is unavailable.

## Acceptance criteria

- [ ] A documented matrix covering the major terminals on macOS, Linux and Windows.
- [ ] An ASCII fallback for terminals without box drawing.
- [ ] Key handling verified on Windows, including the release-event duplication.
