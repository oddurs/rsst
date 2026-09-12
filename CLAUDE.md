# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

`rsst` is a terminal RSS/Atom reader built on ratatui + crossterm, with async
fetching on a tokio runtime. Single binary, no daemon, no database — feeds are
fetched fresh into memory on launch and on `r`.

## Commands

```sh
cargo run                                    # run the TUI
cargo test                                   # unit tests (fast, no network)
cargo clippy --all-targets -- -D warnings    # lint; CI treats warnings as errors
cargo fmt --all                              # format
cargo build --release
```

Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`
before proposing a change is done. CI runs exactly these — but on the latest
stable toolchain, which can flag lints an older local `rustc` doesn't. A green
local clippy is necessary, not sufficient; check the CI run too.

## Layout

| File            | Holds                                                          |
| --------------- | -------------------------------------------------------------- |
| `src/main.rs`   | Terminal setup/teardown, the event loop, key dispatch, fetching |
| `src/app.rs`    | `App` state and all selection/focus logic — pure, no I/O        |
| `src/feed.rs`   | HTTP fetch plus `parse()`, which turns bytes into a `Feed`      |
| `src/config.rs` | TOML config load and starter-file creation                      |
| `src/ui.rs`     | All rendering; reads `App`, never mutates domain state          |

## Conventions that matter here

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
raising it in `Cargo.toml` and `clippy.toml` together.

## Git workflow

Branch `<type>/<description>`, Conventional Commit messages, squash-merge into
`main`. Full details in `CONTRIBUTING.md`. Don't commit or push unless asked.

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

### Releases are labels, not items

`cairn.toml` models releases as a plain `milestone` label (`v0.1` … `v1.0`)
rather than as `[[type]] milestone` items. That is a workaround, and the reason
is written down next to the schema: in cairn 0.2.0 `groups = "one"` does not
create the field it documents, so milestone items fail validation. File work
under a release with `cairn set <ID> milestone=v0.2`.

The narrative intro at the top of `ROADMAP.md` is hand-written in
`docs/roadmap-intro.md`; everything below the generated-by marker comes from the
items and is overwritten on every render. Edit the items, not `ROADMAP.md`.
