---
id: 39
title: Set up the full-auto agentic git workflow
type: chore
status: done
created: 2026-09-11
updated: 2026-09-11
priority: p0
effort: m
area: ci
release: v0.1
---

## Problem

The repository has CI and branch protection, but nothing that lets an agent take an item from the backlog to merged without a human steering each step. The mechanical part — branch, gate, close, commit, PR, merge — is done by hand every time, which is where inconsistency creeps in.

## Proposal

A `scripts/ship` command wrapping the loop, auto-merge enabled on the repository so a green PR lands itself, labels mirroring the cairn schema, and a CI job that fails when the backlog stops matching the schema or `ROADMAP.md` goes stale.

## Acceptance criteria

- [ ] `scripts/ship start <ID>` claims and branches; `finish` gates, closes, commits, pushes, opens a PR and enables auto-merge
- [ ] `finish` refuses to push when fmt, clippy, tests or `cairn check` fail
- [ ] CI fails when `cairn check` fails or `ROADMAP.md` is stale
- [ ] Auto-merge and delete-on-merge are enabled on the repository
- [ ] The loop and its guardrails are documented in CLAUDE.md
