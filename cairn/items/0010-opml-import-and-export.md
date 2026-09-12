---
id: 10
title: OPML import and export
type: feature
status: planned
milestone: v0.1
depends_on:
- 7
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: m
area: config
---

## Problem

Feeds must be added by hand-editing TOML. Every other reader speaks OPML, so there is no migration path in or out — which makes trying rsst a bigger commitment than it should be.

## Proposal

`rsst import <file.opml>` merges feeds into the config, and `rsst export --opml` writes the current list out. Preserve folder structure as tags once tagging exists; ignore it until then.

## Acceptance criteria

- [ ] A NetNewsWire or Feedly OPML export imports without error.
- [ ] Importing twice does not duplicate feeds.
- [ ] Exported OPML re-imports to an identical feed list.
