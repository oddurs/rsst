---
id: 59
title: Entry titles show their HTML entities raw
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p2
release: v1.4
effort: s
area: feed
---

## Problem

A summary goes through `decode_entities`; an entry title does not. The same bytes therefore render two different ways:

```
TITLE="Tom &amp; Jerry &lt;tag&gt;"   SUMMARY="Tom & Jerry <tag>"
```

So any entry whose headline contains an ampersand — "R&amp;D", "Q&amp;A", a band called "&amp;" — reads as `R&amp;D` in the list and in the article header, while the body beneath it reads correctly.

Found by the fixture feeds in `0058`, which is what they are for.

## Proposal

Decode the title where it is built in `feed::parse`, next to the link and the date, and cover it with the same kind of test the summary already has.

Check the feed title too, which comes from the same place and has the same gap.

## Acceptance criteria

- [x] An entry title containing `&amp;`, `&lt;` and a numeric entity renders decoded
- [x] The feed's own title is decoded as well
- [x] A test asserts a title and a summary carrying identical bytes render identically
