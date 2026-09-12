---
id: 7
title: 'Add a real command line: --version and --help'
type: bug
status: planned
milestone: v0.1
created: 2026-09-11
updated: 2026-09-11
priority: p0
effort: s
area: cli
---

## What happens

`rsst` takes no arguments at all. Anything passed is ignored, and there is no `--version` or `--help`.

## What should happen

Standard `--version` and `--help`, plus `--config <path>` to point at an alternative config. Unknown flags should exit non-zero with a usage message.

## Reproduction

1. Run `rsst --version`.
2. Nothing is printed and the TUI starts instead.
