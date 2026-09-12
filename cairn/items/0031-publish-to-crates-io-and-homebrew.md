---
id: 31
title: Publish to crates.io and Homebrew
type: chore
status: backlog
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: m
area: ci
release: v1.0
---

## Problem

Installation means cloning the repository and running `cargo install --path .`. That rules out anyone who does not already have a Rust toolchain.

## Proposal

Publish to crates.io from the release workflow on tag, and maintain a Homebrew tap formula pointing at the release artefacts.

## Acceptance criteria

- [ ] `cargo install rsst` works.
- [ ] `brew install oddurs/tap/rsst` works.
- [ ] Publishing is automated from the tag, not done by hand.
