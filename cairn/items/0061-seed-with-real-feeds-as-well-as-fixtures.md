---
id: 61
title: Seed with real feeds as well as fixtures
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p1
release: v1.5
effort: m
area: dev
---

## Problem

The dev environment is fixtures only. Fixtures are exactly what the publisher chose to write, which makes them good at the awkward cases and useless at the ordinary one: a real feed's markup is messier than anything worth inventing, its entries are longer, its dates are in formats nobody would pick on purpose, and its server has opinions about caching.

So the interface is only ever tried against text written to suit it.

## Proposal

Seed both. Fixtures stay the deterministic core; a set of real, public feeds sits alongside them under their own folder, and the seeder tolerates every one of them being unreachable — offline is the normal state of a test machine, not an error.

## Acceptance criteria

- [x] The seeded environment carries real feeds alongside the fixtures
- [x] Seeding works offline: unreachable real feeds are reported, not fatal
- [x] `scripts/dev seed --offline` skips them entirely, for a deterministic run
- [x] The fixture-only frame stays byte-identical, so determinism is still testable
