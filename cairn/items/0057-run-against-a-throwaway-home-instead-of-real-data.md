---
id: 57
title: Run against a throwaway home instead of real data
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p1
release: v1.4
effort: s
area: cli
---

## Problem

There is no way to point rsst at anything but the one config directory and the one database. `--config` moves the config; the database is always `$XDG_DATA_HOME/rsst/rsst.sqlite3` or the platform equivalent.

So a dev instance writes read and starred state into the reader you actually use, and verifying a change against a fixture means deleting your own database first. Every kind of hands-on testing is either destructive or dishonest.

## Proposal

One environment variable, `RSST_HOME`, that moves every file rsst owns into one directory together. One variable rather than one per file: two would let you set one and forget the other, which is the failure this exists to prevent.

## Acceptance criteria

- [x] `RSST_HOME` moves the config, the database, the read state and the legacy cache
- [x] An empty value reads as "not set", rather than writing into the working directory
- [x] Documented in `--help`, the man page and the README
- [x] A test through the real binary proves the files land in the given home and the platform's own config is left untouched
