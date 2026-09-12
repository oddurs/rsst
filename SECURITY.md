# Security

## Reporting a vulnerability

Please report it privately through GitHub's
[security advisories](https://github.com/oddurs/rsst/security/advisories/new)
rather than opening a public issue.

Include what you did, what happened, and the version (`rsst --version`). A proof
of concept is welcome but not required — a clear description of the problem is
more useful than a working exploit.

Expect an acknowledgement within a week. If a fix is warranted it will be
released and the advisory published; if the report turns out not to be a
vulnerability you will get a reason rather than silence.

## What is in scope

rsst fetches and parses feeds from the network, which is the interesting part of
its attack surface:

- **Feed parsing and article rendering.** A malicious feed should not be able to
  crash the reader, hang it indefinitely, or cause it to write outside its own
  data directory. This is checked rather than asserted: `tests/fuzz.rs` runs a
  committed corpus of malformed feeds and twenty thousand mutations of it
  through the parser, the article renderer and the readability extractor on
  every push, with a time budget per input. It has already found one hang.
- **Network handling.** Requests are made with `rustls` and a fifteen-second
  timeout. Certificate validation is not bypassed anywhere. A response body is
  streamed against a limit (`max_feed_megabytes`, 8 by default) rather than
  buffered whole, so how much memory a fetch costs is rsst's decision and not
  the server's; an oversized `Content-Length` is refused before the body is
  read at all.
- **What gets executed.** Opening an entry hands a URL to the system opener
  (`open`, `xdg-open`, `cmd /C start`). A feed controlling that URL is expected;
  a feed being able to control the *command* would be a vulnerability.
- **Stored state.** The database lives in the platform data directory. Anything
  that writes outside it is a bug worth reporting.

## What is not

- The content of a feed being wrong, misleading or unpleasant. rsst displays
  what publishers publish.
- Denial of service caused by pointing rsst at a deliberately enormous feed on
  your own machine.
- Advisories against transitive dependencies with no reachable path in rsst.
  `cargo audit` runs on every push and on a weekly schedule, so these are
  usually already known.

## Supported versions

Until 1.0, only the latest release is supported.
