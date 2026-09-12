---
id: 44
title: Keep entries and read state in SQLite
type: feature
status: backlog
created: 2026-09-12
updated: 2026-09-12
priority: p1
effort: xl
area: storage
release: v1.1
---

## Problem

Everything rsst persists is a TOML file rewritten whole: the cache on every refresh, read state on every mark. Three consequences, in order of when they bite:

- **Refresh cost is linear in total entries, not changed ones.** Measured: 5,000 entries round-trip in ~14ms. Ten times that is ~140ms on every refresh, and launch parses all of it before drawing.
- **Search is a linear scan** over every entry held in memory, which is how [[0021]] works and is fine only while the whole backlog fits comfortably in memory.
- **Read state only grows.** Three candidate keys per entry read, and nothing prunes it — see [[0009]] for why the keys are plural.

The honest ceiling is somewhere in the tens of thousands of entries: a few hundred feeds kept for a long time.

## Proposal

One SQLite database replacing `feeds.toml` and `read.toml`. Config stays TOML — it is written by hand and belongs in an editor.

- Writes touch only what changed, so refresh cost follows the feed rather than the backlog
- FTS5 for search, so [[0021]] stops being a scan
- Read and starred state become rows that can be pruned rather than an ever-growing list

## Risks worth naming before starting

- **`rusqlite` bundles SQLite's C source.** The release workflow cross-compiles `aarch64-unknown-linux-gnu` on an x86 runner; it already installs `gcc-aarch64-linux-gnu` for the linker, and that needs to be enough to compile C for the target too. Verify this early — it decides whether the approach is viable at all.
- **`docs/stability.md` commits both formats to a versioned migration path.** The existing TOML must be read and carried over, not discarded: read state is the reader's own history and is not reproducible.
- This supersedes [[0043]]. Doing both in sequence is wasted work, so one of them should be dropped rather than left to rot.

## Acceptance criteria

- [ ] `feeds.toml` and `read.toml` are migrated into the database, not discarded
- [ ] Refreshing one feed writes only that feed's rows
- [ ] Search uses FTS5 and no longer scans every entry in memory
- [ ] Read and starred state are pruned when an entry is neither cached nor starred
- [ ] A corrupt or missing database is recreated rather than fatal, as the TOML was
- [ ] All five release targets still build, including cross-compiled aarch64 Linux
- [ ] The benchmark shows refresh cost no longer scaling with total entries
