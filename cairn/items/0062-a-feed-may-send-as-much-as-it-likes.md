---
id: 62
title: A feed may send as much as it likes
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p0
release: v1.5
effort: m
area: net
---

## Problem

`fetch` calls `response.bytes()`, which buffers the whole body with no limit. Nothing anywhere caps it — not a `Content-Length` check, not a streaming cut-off.

Measured against a server that simply keeps writing:

| body sent | peak resident |
| --------- | ------------- |
| 300 MB    | 865 MB        |
| 600 MB    | 1.6 GB        |

Linear, and roughly 2.7x the bytes on the wire. A server that sends a few gigabytes takes the reader down with it, and `SECURITY.md` promises the opposite:

> a malicious feed cannot crash the reader, hang it indefinitely, or cause it to write outside its own data directory

It is not only hostility. A misconfigured server that streams an error page forever does the same thing by accident.

## Proposal

Read the body as a stream with a hard ceiling, and refuse past it with an error that names the limit. Reject early on a `Content-Length` that already exceeds it, rather than downloading to find out.

The ceiling wants to be generous — real feeds reach a few megabytes — and configurable for someone who knows their feed is bigger.

## Acceptance criteria

- [x] A body over the limit fails with an error naming the limit, rather than being buffered
- [x] An oversized `Content-Length` is refused before the body is read
- [x] Peak memory stays bounded no matter how much the server sends
- [x] The limit is configurable, with a sane default
- [x] A test serves more than the limit and asserts the refusal

## 2026-09-12

Measured before: 300MB body -> 865MB resident, 600MB -> 1.6GB, linear. After: 600MB -> 23MB, and the server sees the connection reset mid-body rather than rsst draining it politely.
