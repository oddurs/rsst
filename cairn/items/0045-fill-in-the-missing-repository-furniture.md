---
id: 45
title: Fill in the missing repository furniture
type: chore
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p2
effort: s
area: ci
release: v1.0
---

## Problem

The repository has CI, branch protection, templates, labels and a release pipeline, but a few things a public project is expected to have are missing:

- no `CHANGELOG.md`, so there is nowhere for a release to say what changed
- no `SECURITY.md`, so someone finding a vulnerability has to guess how to report it
- no `CODE_OF_CONDUCT.md`
- no `.editorconfig`, so contributors' editors disagree about whitespace before `cargo fmt` gets a chance
- no `CODEOWNERS`, and no issue-template chooser config
- GitHub does not detect the licence, and the repository has no homepage set

## Proposal

Add them. None is interesting on its own; the point is that a public repository should answer these questions without anyone having to ask.

## Acceptance criteria

- [x] `CHANGELOG.md` exists and covers the work done so far
- [x] `SECURITY.md` says how to report a vulnerability and what is supported
- [x] `CODE_OF_CONDUCT.md` exists
- [x] `.editorconfig` matches what `cargo fmt` and the repository already do
- [x] `CODEOWNERS` and an issue-template chooser exist
- [x] GitHub detects the licence, and the repository has a homepage
