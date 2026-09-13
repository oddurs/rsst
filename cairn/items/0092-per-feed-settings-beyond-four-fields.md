---
id: 92
title: Per-feed settings beyond four fields
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

A feed carries four settings: `url`, `title`, `tags`, `refresh_minutes`. Everything else is global.

So a feed that publishes teasers needs `f` pressed on every entry, when it should simply always be fetched in full. A comics feed wants oldest-first while the news wants newest. A noisy feed wants its own rules. None of that can be said.

## Proposal

Per-feed overrides for the settings where feeds genuinely differ: always fetch the full article, sort, retention, and a filter that applies to that feed alone. Each falls back to the global value, and the config documents which.

## Acceptance criteria

- [ ] A feed can be set to always fetch the full article
- [ ] A feed can override sort and retention
- [ ] Every override falls back to the global setting when unset
- [ ] `docs/config.example.toml` documents each, and the drift guard covers them
