---
id: 31
title: Publish to crates.io and Homebrew
type: chore
status: blocked
assignee: Oddur Sigurdsson
claimed: 2026-09-12
created: 2026-09-11
updated: 2026-09-12
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
- [x] Publishing is automated from the tag, not done by hand.

## 2026-09-12

Automation is done and the formula template renders to valid Ruby; the two install criteria are blocked on decisions and credentials that are not mine to make.

**The crates.io name is taken.** `rsst` belongs to an unrelated tool ('a commandline tool that dump articles in followed feeds into offline files'), and `cargo publish --dry-run` confirms it: 'crate rsst@0.1.0 already exists on crates.io index'. So `cargo install rsst` would install somebody else's program. Available alternatives checked: rsst-tui, rsst-reader, rsstr, rsst-rs, feedrs, tuirss. Renaming the published crate is a naming decision for the owner, not something to pick unilaterally.

**Both publish jobs need secrets that do not exist yet**: CARGO_REGISTRY_TOKEN, and HOMEBREW_TAP_TOKEN plus an oddurs/homebrew-tap repository. Each job is gated on a repository variable (PUBLISH_TO_CRATES_IO, UPDATE_HOMEBREW_TAP) so a tag still produces binaries for anyone not publishing.

Remaining: pick a crates.io name, add the two secrets, create the tap repo, then tag a release. The package itself is ready — 37 files, 100 KB compressed, verified by a dry run.
