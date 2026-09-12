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

Trying anything by hand means waiting on real feeds over the real network. They are different on every run, they need the internet, and they cannot be made to produce the cases that actually break a reader: a feed that 404s, one that is not XML at all, an entry with a 300-line code block, a title in Arabic, ten thousand entries, an article that is nothing but a table.

So "is this still right?" gets answered by clicking around whatever the internet happened to publish that morning. There is no way to put a known interface on screen twice.

## Proposal

A throwaway environment that needs no network and is the same every time, built on the home from `0057`:

- Fixture feeds in the repository, served over a local HTTP server, so the real fetch, parse and conditional-request paths run rather than being bypassed
- A seeded database, built by a command, carrying read, starred and folder state worth looking at
- One script that creates the environment, seeds it, and runs the reader in it
- Written down in `CONTRIBUTING.md` and `CLAUDE.md`, so it is the obvious way to try a change

## Acceptance criteria

- [ ] `scripts/dev` brings up a seeded environment and runs the reader in it, with no network
- [ ] The fixtures cover the cases that break a reader: errors, huge entries, unicode, malformed markup, empty feeds
- [ ] Re-running it gives the same interface, so a screenshot can be compared against the last one
- [ ] It cannot touch real data, and a test says so
- [ ] Documented where a contributor will find it
