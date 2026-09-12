# Stability

What rsst promises, and what it does not. Until 1.0, treat all of it as
intention rather than guarantee; from 1.0 it is the contract.

## What semver covers

A **major** release may change any of these. A **minor** release may add to
them. A **patch** release changes none of them.

| Covered | Meaning |
| ------- | ------- |
| **Config schema** | Keys in `config.toml`, their types, and their defaults. A config that works on 1.0 works on every 1.x. |
| **Key bindings** | The default binding for an action, and the action names used in `[keys]`. |
| **CLI** | Flags, subcommands, and their exit codes. |
| **On-disk formats** | `read.toml` and `feeds.toml` — see below. |
| **OPML** | That an export re-imports to the same feed list. |

Deliberately **not** covered: the exact wording of messages, the layout of the
panes, the colours a theme resolves to, and anything printed for humans rather
than parsed by programs.

## On-disk formats

Both state files carry a `version` field:

| File | Holds | Current |
| ---- | ----- | ------- |
| `read.toml` | What has been read and starred | 1 |
| `feeds.toml` | Cached feed contents and HTTP validators | 1 |

The rules are asymmetric on purpose:

- **An older file is migrated forward.** Read state is the reader's own history
  and is not reproducible — losing it is a real loss, so it is carried across
  format changes rather than discarded.
- **A newer file is discarded.** Downgrading rsst must not silently misread a
  file whose fields mean something else. For the cache this costs one refetch.
  For read state it costs the history, which is unfortunate but better than
  marking an unread backlog read on a guess.
- **A corrupt or truncated file is discarded**, never fatal. Both files are
  written to a temporary sibling and renamed into place, so a crash mid-write
  leaves the previous version intact.

A format change means bumping the constant, adding a branch to `migrate`, and a
test that a file in the old format still loads.

## Minimum supported Rust version

The MSRV is declared in `Cargo.toml` (`rust-version`) and `clippy.toml`, and is
checked by the `msrv` job in CI against that exact toolchain. Both files must
agree; the CI job is what makes the claim true rather than aspirational.

- **Raising the MSRV is a minor release**, never a patch.
- It is raised when a language or library feature genuinely earns it, not
  because a newer compiler exists.
- The bump is its own `chore:` commit, so it is visible in the history and can
  be reverted on its own.

## Deprecation

A covered thing that is going away keeps working for at least one minor release
while warning, and is removed no earlier than the next major. Anything rsst
stops accepting, it says so about — silently ignoring a key someone put in their
config is worse than refusing it.
