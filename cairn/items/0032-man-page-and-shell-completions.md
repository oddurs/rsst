---
id: 32
title: Man page and shell completions
type: docs
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p1
effort: s
area: docs
release: v1.0
---

## Problem

There is no man page, and no completions for any shell.

## Proposal

Generate both from the CLI definition at build time and ship them in the release archives.

## Acceptance criteria

- [x] `man rsst` works after install.
- [x] Completions work in fish, bash and zsh.
- [x] Both are generated, not hand-written.
