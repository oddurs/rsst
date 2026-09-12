---
id: 28
title: Theme and colour configuration
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

Colours are hardcoded to cyan and dark grey, which fights many terminal themes and is hard to read on a light background.

## Proposal

A `[theme]` config table, a light and a dark preset, and honouring NO_COLOR.

## Acceptance criteria

- [ ] A theme from the config changes the UI.
- [ ] NO_COLOR produces a monochrome UI.
- [ ] The bundled presets are legible on both light and dark backgrounds.
