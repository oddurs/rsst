---
id: 26
title: Group feeds into folders or tags
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p1
effort: m
area: config
release: v0.4
---

## Problem

The feed list is flat. At fifty feeds it is a wall of names with no structure and no way to read just one topic.

## Proposal

Optional tags per feed in the config, with the feed pane grouping by tag and collapsible groups. Maps onto OPML folders in both directions.

## Acceptance criteria

- [x] Feeds group under their tags and groups collapse.
- [x] An untagged feed still appears.
- [x] OPML import preserves folders as tags, and export reverses it.
