`rsst` is a terminal RSS/Atom reader. Today it fetches feeds, lists entries and
shows one at a time — enough to demonstrate the shape, not yet enough to replace
whatever you read feeds in now. This is the path from one to the other.

Each release below is a coherent claim about what the reader can do, not a
grab-bag of tickets. The dates are intentions rather than commitments.

| Release | Theme | Target |
| ------- | ----- | ------ |
| **v0.1** | **Usable daily** — read state that survives a restart, feeds you can import, entries you can open | 2026-10-15 |
| **v0.2** | **Fast and offline** — launch from cache, refresh in the background, stop refetching what hasn't changed | 2026-11-30 |
| **v0.3** | **Reading and navigation** — search, unread filtering, bulk marking, keys where a vim user expects them | 2027-01-31 |
| **v0.4** | **Make it yours** — keybindings, colours and feed organisation under the reader's control | 2027-03-31 |
| **v1.0** | **Ready to recommend** — packaged, documented, and stable enough to depend on | 2027-06-30 |

Two defects are scheduled first because they undercut everything else: there is
no `--version` or `--help` at all, and a panic leaves the terminal in raw mode
with the alternate screen still active.
