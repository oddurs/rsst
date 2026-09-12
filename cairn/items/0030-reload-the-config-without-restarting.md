---
id: 30
title: Reload the config without restarting
type: feature
status: backlog
created: 2026-09-11
updated: 2026-09-11
priority: p3
effort: s
area: config
release: v0.4
---

## Problem

Adding a feed means quitting, editing TOML, and starting over.

## Proposal

`R` re-reads the config and reconciles the feed list, keeping state for feeds that are still present.

## Acceptance criteria

- [ ] Adding a feed and pressing `R` picks it up without a restart.
- [ ] Read state survives the reload.
- [ ] A config that fails to parse leaves the running state untouched and reports the error.
