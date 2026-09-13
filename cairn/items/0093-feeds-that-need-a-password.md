---
id: 93
title: Feeds that need a password
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

Some feeds are behind HTTP authentication: a paid subscription, a private Miniflux instance, an internal feed at work. rsst sends no credentials and shows `!`, with `0063` correctly reporting that the server refused the request.

## Proposal

Per-feed credentials, kept out of the config file — a config full of passwords is a config nobody can share, paste into an issue, or put in a dotfiles repository.

The right shape is a reference: the config names where the secret lives (an environment variable, a command to run, the system keychain), and rsst resolves it at fetch time. A command is the most portable and the most flexible, and is what `pass` users already expect.

## Acceptance criteria

- [ ] A feed can carry basic auth credentials resolved at fetch time
- [ ] The secret never has to be written in the config
- [ ] A credential that cannot be resolved fails that feed with a clear reason, not a crash
- [ ] Credentials never reach the status line, a log, or an error message
- [ ] `SECURITY.md` says what is stored and what is not
