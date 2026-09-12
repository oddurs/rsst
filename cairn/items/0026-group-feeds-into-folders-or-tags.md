---
id: 26
title: Group feeds into folders or tags
type: feature
status: backlog
milestone: v0.4
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: m
area: config
---

## Problem

The feed list is flat. At fifty feeds it is a wall of names with no structure and no way to read just one topic.

## Proposal

Optional tags per feed in the config, with the feed pane grouping by tag and collapsible groups. Maps onto OPML folders in both directions.

## Acceptance criteria

- [ ] Feeds group under their tags and groups collapse.
- [ ] An untagged feed still appears.
- [ ] OPML import preserves folders as tags, and export reverses it.
