---
id: 38
title: Automate dependency updates
type: chore
status: backlog
milestone: v1.0
created: 2026-09-11
updated: 2026-09-11
priority: p2
effort: s
area: ci
---

## Problem

Dependencies are updated by hand, which in practice means when something breaks. The ratatui 0.29 advisories sat in the very first CI run.

## Proposal

Dependabot or Renovate for cargo and GitHub Actions, grouped into a weekly pull request, plus a scheduled `cargo audit` so advisories surface without waiting for a push.

## Acceptance criteria

- [ ] Dependency pull requests open automatically on a schedule.
- [ ] Patch updates are grouped rather than one PR each.
- [ ] A scheduled audit run reports new advisories against an unchanged tree.
