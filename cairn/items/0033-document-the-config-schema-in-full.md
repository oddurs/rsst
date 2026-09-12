---
id: 33
title: Document the config schema in full
type: docs
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p2
effort: s
area: docs
release: v1.0
---

## Problem

The README shows two example feed entries. Every other key — themes, keys, tags, limits — is undocumented and discoverable only by reading the source.

## Proposal

A reference page covering every key, its default, and its accepted values, with a fully populated example config.

## Acceptance criteria

- [x] Every key the parser accepts is documented.
- [x] Defaults are stated.
- [x] The example config parses — enforced by a test.
