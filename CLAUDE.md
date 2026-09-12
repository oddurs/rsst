# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

`rsst` is a terminal RSS/Atom reader built on ratatui + crossterm, with async
fetching on a tokio runtime. Single binary, no daemon, no database — feeds are
fetched fresh into memory on launch and on `r`.

## Commands

```sh
cargo run --bin rsst                         # run the TUI
cargo run --release --bin rsst-bench         # timings vs benches/baseline.toml
cargo test                                   # unit tests (fast, no network)
cargo clippy --all-targets -- -D warnings    # lint; CI treats warnings as errors
cargo fmt --all                              # format
cargo build --release
```

Benchmarks compare against `benches/baseline.toml` and fail past a 6x tolerance.
That is sized to catch an operation that has stopped being linear, not one that
is 20% slower — regenerate the baselines only when a change is understood.

Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`
before proposing a change is done. CI runs exactly these — but on the latest
stable toolchain, which can flag lints an older local `rustc` doesn't. A green
local clippy is necessary, not sufficient; check the CI run too.

## Layout

`rsst` is a library plus two binaries. Everything lives in `src/lib.rs` so the
benchmark — and any future integration test — can use the real code rather than
a copy of it; `src/main.rs` is the reader, `src/bin/bench.rs` the benchmark.

| File            | Holds                                                          |
| --------------- | -------------------------------------------------------------- |
| `src/main.rs`   | Terminal setup/teardown, the event loop, key dispatch, fetching |
| `src/app.rs`    | `App` state and all selection/focus logic — pure, no I/O        |
| `src/feed.rs`   | HTTP fetch plus `parse()`, which turns bytes into a `Feed`      |
| `src/config.rs` | TOML config load and starter-file creation                      |
| `src/article.rs`| Parses an entry's HTML into blocks and lays them out            |
| `src/ui.rs`     | All rendering; reads `App`, never mutates domain state          |

## Conventions that matter here

**`article.rs` decides structure; `ui.rs` decides appearance.** The parser says
"this is a list item", the layout says where the text goes, and only the
renderer picks a glyph or a colour. A bullet chosen during parsing is a bullet
that cannot become an asterisk on a terminal that has no bullet — which is
exactly the bug the ASCII test caught.

**Keep logic out of `ui.rs`.** It has smoke tests against ratatui's
`TestBackend` that assert text reaches the screen, but that is all they can
cheaply cover. Anything with a decision in it belongs in `app.rs` or `feed.rs`,
which are pure and thoroughly tested. When adding a feature, ask what part of it
can be a function over plain data, and put that part there first.

**`parse()` is split from `fetch()` on purpose** so feed handling is testable
without a network. New parsing behaviour gets a test against an inline XML
fixture in `src/feed.rs` — don't add a test that hits the network.

**One dead feed must not break the reader.** `fetch_all` turns a failed fetch
into a placeholder `Feed` marked `(error)`. Preserve that: never `?` out of the
fetch path in a way that aborts the whole refresh.

**Restore the terminal on every exit path.** `main` calls `leave()` after `run()`
returns, whatever the result. If you add an early return or a panic path, make
sure raw mode and the alternate screen still get undone — a half-exited TUI
leaves the user's shell unusable.

**The event loop is `poll` + `read`.** Only handle `KeyEventKind::Press`;
Windows delivers a Release for every key and will double every action otherwise.

**Selection wraps.** `app::step` wraps at both ends and returns 0 for an empty
list. Reuse it rather than writing new bounds arithmetic.

## Dependencies

Adding a crate needs a reason beyond convenience — this is a small binary and
the dependency tree is deliberately short. Notably: there is no `futures`
dependency; concurrent fetching uses `tokio::task::JoinSet`. `reqwest` uses
`rustls`, not the system OpenSSL — keep `default-features = false`. `ratatui` is
pinned to the features we use for the same reason; its defaults add the calendar
widget and `ratatui-macros`, which this app never touches.

MSRV is 1.90, checked in CI. Don't reach for newer language features without
raising it in `Cargo.toml` and `clippy.toml` together — and note that raising it
is a minor release, per `docs/stability.md`.

**State lives in SQLite** (`src/db.rs`), versioned by `user_version`. An older
schema is migrated forward; a newer one is refused rather than misread. Changing
it means bumping `SCHEMA`, adding a branch to `migrate`, and a test that a
database at the old version still opens.

Read and starred keys are also held in memory: rendering asks "is this read?"
once per visible row per frame, and a query per cell would be absurd. The sets
are loaded once and written back as **deltas**, never as a whole-table rewrite —
that rewrite is exactly what the TOML files did and why they did not scale.

`src/cache.rs` and `ReadState::load` survive only to migrate the old TOML files
in. Nothing else should use them.

## Fuzzing

`tests/fuzz.rs` runs a committed corpus and deterministic mutations of it
through everything that touches untrusted bytes. Deterministic rather than
coverage-guided on purpose: `cargo-fuzz` needs nightly, and a check that cannot
run in CI is a check that rots. The seed is fixed, so a failure reproduces
exactly; `RSST_FUZZ_ITERATIONS` turns it up for a longer run by hand.

Anything it finds gets fixed **and** keeps a regression test. It found the
readability extractor matching each candidate's closing tag by scanning forward
from it — quadratic in nesting depth, and 15 seconds on a page nested 50,000
levels deep, which a hostile page can choose to be.

## Git workflow

Branch `<type>/<description>`, Conventional Commit messages, squash-merge into
`main`. Full details in `CONTRIBUTING.md`.

### Shipping an item

One item, one branch, one pull request. `scripts/ship` does the mechanical part
so it happens the same way every time:

```sh
scripts/ship start 12     # claim it, branch from a fresh main
# ... do the work, with tests ...
scripts/ship finish 12    # gate, close, commit, push, PR, auto-merge
```

`finish` runs fmt, clippy, tests and `cairn check` **before** it pushes, so a red
branch never becomes a pull request. It then closes the item, re-renders
`ROADMAP.md`, generates the PR body from the item, and enables auto-merge — the
PR lands itself once CI is green. Nothing waits on a human.

Work one item at a time and let each land before starting the next. Two open
pull requests that both touch `cairn/items` will conflict on `ROADMAP.md`; the
merge driver from `cairn init --git` resolves it, but only in a working copy
where that hook is installed, and never in CI.

If the work turns out to be bigger than the item, stop and split it: `cairn new`
for the part you are not doing, then `cairn note` on the original saying why.
Don't silently widen a branch.

### Don't

- Don't commit to `main`. It is protected — PR, green CI, squash.
- Don't hand-edit `ROADMAP.md`; it is generated. Edit the items.
- Don't merge with `--admin` or force-push a shared branch.
- Don't close an item you have not actually finished. `cairn note` what is left.

<!-- cairn:begin -->
## Roadmap and issues

This project tracks its roadmap and issues with `cairn`. Every item is a Markdown file under `cairn/items`, described by the schema in `cairn.toml`.

**Do not create ad-hoc TODO, PLAN or NOTES files.** Create a cairn item instead, so the work appears on the board and in the generated roadmap.

### The loop

1. `cairn next` — what is ready to start. It excludes anything blocked by unfinished dependencies and puts work already in progress first.
2. `cairn claim <ID>` — take it before you start, so no one duplicates the work. `cairn claim --next` picks and claims the top-ranked unclaimed item in one step, and prints its body so you can begin immediately.
3. Do the work. Record what you learn: `cairn set <ID> <field>=<value>` for fields, `cairn note <ID> "<TEXT>"` for anything that needs a sentence — why you chose something, what you tried, what to watch for.
4. `cairn close <ID>` when it is done, or `cairn release <ID>` to hand it back.
5. `cairn check` before you report finished. It must pass.

### Commands

```sh
cairn next --json                 # ready work, ranked
cairn claim --next                # take the next ready item
cairn search <TEXT> --json        # titles, bodies and labels
cairn list --json                 # all open items
cairn list --filter 'blocked=false,priority=p0'
cairn show <ID> --json            # one item, including its body
cairn new "<TITLE>" --type <TYPE> --milestone <MILESTONE>
cairn set <ID> status=<STATUS>    # also labels+=x, or any field below
cairn note <ID> "<TEXT>"          # append reasoning; never replaces
cairn close <ID>
cairn check                       # validate; run before finishing
cairn render                      # regenerate ROADMAP.md
```

### Schema

- **Types**: `feature`, `bug`, `chore`, `docs`
- **Statuses**: `backlog` (open), `planned` (open), `doing` (active), `blocked` (active), `done` (done), `dropped` (dropped)
- **`due`**: date, YYYY-MM-DD — when a milestone is meant to land
- **`part_of`**: names any items, by id, several allowed — a larger piece of work this belongs to
- **`priority`**: one of p0, p1, p2, p3 — p0 is a release blocker
- **`effort`**: one of s, m, l, xl — Rough size, not an estimate
- **`area`**: free text — Subsystem this touches
- **Saved views** (`cairn list --view NAME`): `now`, `next`, `triage`

### Rules

1. Before starting work, find or create the item and set it to an active status.
2. Use the fields above rather than inventing new ones; add new fields to `cairn.toml` first.
3. Never hand-edit the generated roadmap file — change items and run `cairn render`.
4. `cairn check` must pass before the work is considered done.

<!-- cairn:end -->

### Releases are a `release` field, not milestone items

`cairn.toml` files work under a declared `release` enum (`v0.1` … `v1.0`) rather
than under `[[type]] milestone` items. That is a workaround for cairn 0.2.0, and
the reason is written down next to the schema: `groups = "one"` does not create
the field it documents, and the built-in `milestone` label that does work is
undeclared, so nothing can group by it — `ROADMAP.md` falls back to status and
harrow says "nothing to group by called milestone".

File work under a release with `cairn set <ID> release=v0.2`. Do not use
`milestone=`; it still parses and will quietly leave the item ungrouped.

The narrative intro at the top of `ROADMAP.md` is hand-written in
`docs/roadmap-intro.md`; everything below the generated-by marker comes from the
items and is overwritten on every render. Edit the items, not `ROADMAP.md`.
