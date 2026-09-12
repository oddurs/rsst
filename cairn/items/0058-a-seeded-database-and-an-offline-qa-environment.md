---
id: 58
title: A seeded database and an offline QA environment
type: feature
status: backlog
created: 2026-09-12
updated: 2026-09-12
priority: p1
release: v1.4
effort: m
area: dev
part_of:
- 57
---

## Problem

## Proposal

## Acceptance criteria

- [ ]

## Problem

Trying anything by hand means waiting on real feeds over the real network. They are different on every run, they need the internet, and they cannot be made to produce the cases that actually break a reader: a feed that 404s, one that redirects, an entry with a 300-line code block, a title in Arabic, ten thousand entries, an article that is nothing but a table.

So "is this still right?" gets answered by clicking around whatever the internet happened to publish that morning. There is no way to put a known interface on screen twice.

## Proposal

A throwaway environment that needs no network and is the same every time:

- Fixture feeds in the repository, served over a local HTTP server, so the real fetch, parse and conditional-request paths run
- A seeded database, built by a command, covering the cases worth looking at
- One script that creates the environment, seeds it and runs the reader in it
- Written down, so it is the obvious way to try a change
