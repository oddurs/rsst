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
before proposing a change is done. CI runs exactly these.

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
`rustls`, not the system OpenSSL — keep `default-features = false`.

MSRV is 1.90, checked in CI. Don't reach for newer language features without
raising it in `Cargo.toml` and `clippy.toml` together.

## Git workflow

Branch `<type>/<description>`, Conventional Commit messages, squash-merge into
`main`. Full details in `CONTRIBUTING.md`. Don't commit or push unless asked.
