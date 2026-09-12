use std::cmp::Reverse;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::FeedSource;

/// A fetched feed and its entries, ready for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub title: String,
    pub url: String,
    pub entries: Vec<Entry>,
    /// Where this feed's last fetch got to. Never cached: a feed restored from
    /// disk is not in flight, and yesterday's error is not today's.
    #[serde(skip)]
    pub status: Status,
}

/// Why a fetch failed, to the degree that changes what to do about it.
///
/// The distinctions are behavioural, not descriptive: two causes share a
/// variant when nothing would treat them differently. Coming back later is the
/// question this exists to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// The host was never reached: DNS, a refused connection, a TLS failure.
    Unreachable,
    /// The request was made and took too long.
    TimedOut,
    /// The server answered with an error it may well recover from.
    ServerError(u16),
    /// The server answered with an error it will not recover from.
    Refused(u16),
    /// There is no feed here, and there is not going to be one.
    Gone,
    /// More was sent than we agreed to read.
    TooBig,
    /// Bytes arrived, and they are not a feed.
    NotAFeed,
}

impl Trouble {
    /// Whether coming back shortly could plausibly work.
    ///
    /// A timeout or an unreachable host is usually a network that will be back.
    /// A 404 is a decision someone made.
    pub fn transient(self) -> bool {
        match self {
            Self::Unreachable | Self::TimedOut | Self::ServerError(_) => true,
            Self::Refused(_) | Self::Gone | Self::TooBig | Self::NotAFeed => false,
        }
    }

    /// How this reads in the status line, in words rather than in jargon.
    pub fn sentence(self) -> String {
        match self {
            Self::Unreachable => "could not be reached".into(),
            Self::TimedOut => "took too long to answer".into(),
            Self::ServerError(code) => format!("server error {code}"),
            Self::Refused(code) => format!("refused the request ({code})"),
            Self::Gone => "is no longer there".into(),
            Self::TooBig => "sent more than the limit allows".into(),
            Self::NotAFeed => "did not send a feed".into(),
        }
    }
}

/// A failed fetch: what kind, and what to show.
#[derive(Debug, Clone)]
pub struct Failure {
    pub trouble: Trouble,
    /// The detail, for someone who wants to know more than the sentence.
    pub detail: String,
}

impl Failure {
    fn new(trouble: Trouble, detail: impl Into<String>) -> Self {
        Self {
            trouble,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl std::error::Error for Failure {}

/// What happened, or is happening, to a feed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Status {
    /// Fetched, or restored from cache and not currently being fetched.
    #[default]
    Idle,
    /// A request is in flight.
    Fetching,
    /// The last attempt failed. Carries the kind, so callers can decide, and
    /// the message, which is the only place the detail survives.
    Failed { trouble: Trouble, message: String },
}

impl Status {
    /// The marker shown beside the feed's name.
    pub fn marker(&self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Fetching => Some("…"),
            Self::Failed { .. } => Some("!"),
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { message, .. } => Some(message),
            _ => None,
        }
    }

    /// What kind of trouble, when there is any.
    pub fn trouble(&self) -> Option<Trouble> {
        match self {
            Self::Failed { trouble, .. } => Some(*trouble),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub title: String,
    pub link: Option<String>,
    pub published: Option<DateTime<Utc>>,
    /// The entry's text, with the markup removed — for the list and for
    /// full-text search.
    pub summary: String,
    /// The entry's markup as published, for the article renderer. Kept apart
    /// from `summary` because the two answer different questions: one is for
    /// matching, the other for reading.
    #[serde(default)]
    pub content: String,
    /// Every identifier this entry could reasonably be recognised by, best
    /// first. Read state matches on any of them — see [`crate::state`].
    pub keys: Vec<String>,
}

impl Entry {
    /// `YYYY-MM-DD`, or an em dash when the feed omitted a date.
    pub fn date_label(&self) -> String {
        self.published
            .map(|when| when.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".into())
    }
}

impl Feed {
    /// The placeholder shown for a feed whose first fetch is still in flight.
    ///
    /// It carries the configured title so the feed list has its final shape
    /// immediately, rather than rearranging itself as results land.
    pub fn pending(source: &FeedSource) -> Self {
        Self {
            title: source.title.clone().unwrap_or_else(|| source.url.clone()),
            url: source.url.clone(),
            entries: Vec::new(),
            status: Status::Fetching,
        }
    }
}

/// What a conditional fetch produced.
#[derive(Debug)]
pub enum Outcome {
    /// New content, with whatever validators the response carried.
    Updated {
        feed: Box<Feed>,
        etag: Option<String>,
        last_modified: Option<String>,
        /// Where the feed turned out to live, when that was not where we
        /// looked — a page named it in a `<link rel="alternate">`.
        found_at: Option<String>,
    },
    /// The server confirmed nothing has changed. Nothing was downloaded or
    /// reparsed; what is already on screen stands.
    NotModified,
    /// The server asked us to back off for this long.
    RateLimited { retry_after: Duration },
}

/// The most a feed may send before we stop listening.
///
/// Generous: the largest feeds in ordinary use are a few megabytes, and a
/// reader that refuses a real feed is worse than one that accepts a silly one.
/// The point is only that the number exists, because without it the ceiling is
/// whatever the server feels like sending.
pub const DEFAULT_MAX_BODY: usize = 8 * 1024 * 1024;

/// What a fetch is allowed to do.
///
/// A struct rather than another argument, because this is where the rest of
/// the engine's policy is going to live.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Bytes of response body, after any transfer encoding is undone.
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body: DEFAULT_MAX_BODY,
        }
    }
}

/// Reads a response body, stopping if it goes past `limit`.
///
/// Streamed rather than buffered whole: `bytes()` allocates whatever arrives,
/// so memory was previously the server's decision. A body of 600 MB took 1.6 GB
/// of resident memory to receive, and nothing stopped it going further.
async fn body_within(
    mut response: reqwest::Response,
    limit: usize,
    url: &str,
) -> std::result::Result<Vec<u8>, Failure> {
    let too_big = |seen: u64| {
        Failure::new(
            Trouble::TooBig,
            format!("{url} sent {seen} bytes, over the {limit} byte limit"),
        )
    };

    // A declared length over the limit is refused before anything is read.
    // Downloading a gigabyte to discover it is a gigabyte is the mistake.
    if let Some(declared) = response.content_length()
        && declared > limit as u64
    {
        return Err(too_big(declared));
    }

    // Sized to what the server declared, clamped — not to the limit, which
    // would reserve megabytes per feed for feeds that are mostly tiny.
    let expected = response
        .content_length()
        .unwrap_or(0)
        .min(limit as u64)
        .min(1 << 20) as usize;
    let mut body: Vec<u8> = Vec::with_capacity(expected);

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| Failure::new(trouble_for(&err), format!("reading body of {url}: {err}")))?
    {
        if body.len() + chunk.len() > limit {
            return Err(too_big((body.len() + chunk.len()) as u64));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// When to come back after a failure that might not repeat.
///
/// Bounded in both attempts and total time: a feed that is down should cost a
/// few seconds and then be left alone, not occupy a lane for a minute while
/// the rest of the list waits behind it.
#[derive(Debug, Clone, Copy)]
pub struct Retry {
    /// Attempts *after* the first. Zero never retries.
    pub attempts: u32,
    /// The wait before the first retry; each one after doubles.
    pub first_delay: Duration,
    /// No single wait longer than this.
    pub max_delay: Duration,
    /// Give up once this much has been spent, however many attempts remain.
    pub max_total: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            attempts: 2,
            first_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(5),
            max_total: Duration::from_secs(20),
        }
    }
}

impl Retry {
    /// How long to wait before attempt number `attempt`, or `None` to stop.
    ///
    /// `attempt` counts retries, so the first retry is 1. `spent` is how long
    /// this feed has already taken, which is what bounds a slow failure.
    pub fn delay(&self, attempt: u32, url: &str, spent: Duration) -> Option<Duration> {
        if attempt > self.attempts || spent >= self.max_total {
            return None;
        }
        let doubled = self
            .first_delay
            .saturating_mul(1u32.checked_shl(attempt - 1).unwrap_or(u32::MAX))
            .min(self.max_delay);
        let wait = doubled + spread(url, attempt, doubled);

        // Never wait past the budget — better to give up now than to sleep
        // through the time and then give up anyway.
        let left = self.max_total.checked_sub(spent)?;
        Some(wait.min(left))
    }
}

/// A per-feed offset, so feeds on one host do not all come back at once.
///
/// Derived from the URL rather than from a random source: it needs no
/// dependency, it is the same on every run — which makes it testable — and it
/// spreads feeds apart, which is the only property that actually matters.
fn spread(url: &str, attempt: u32, base: Duration) -> Duration {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash = hash.wrapping_add(u64::from(attempt).wrapping_mul(0x9e37_79b9_7f4a_7c15));

    // Up to half the wait again, so the spread grows with the wait itself.
    let half = (base.as_millis() as u64) / 2;
    if half == 0 {
        return Duration::ZERO;
    }
    Duration::from_millis(hash % half)
}

/// Downloads and parses a feed, asking the server to skip it if unchanged.
pub async fn fetch(
    client: &reqwest::Client,
    source: &FeedSource,
    etag: Option<&str>,
    last_modified: Option<&str>,
    limits: Limits,
) -> std::result::Result<Outcome, Failure> {
    fetch_inner(client, source, etag, last_modified, limits, true).await
}

/// How many redirects to follow before deciding it is a loop.
///
/// Redirects are followed here rather than by `reqwest` because the *kind*
/// matters: a permanent one is a server saying "stop asking here", and
/// following it silently means paying the extra round trip forever.
const MAX_REDIRECTS: usize = 5;

/// The fetch itself. `may_discover` is false on the second request, so a page
/// pointing at a page is a failure rather than the start of a chain.
async fn fetch_inner(
    client: &reqwest::Client,
    source: &FeedSource,
    etag: Option<&str>,
    last_modified: Option<&str>,
    limits: Limits,
    may_discover: bool,
) -> std::result::Result<Outcome, Failure> {
    let mut hops = 0usize;
    let mut target = source.url.clone();
    // Stays true only while every hop so far has been permanent: one temporary
    // hop anywhere in the chain makes the final address a temporary answer.
    let mut permanently_moved = false;
    let mut moved = false;

    let (response, status) = loop {
        let mut request = client.get(&target);
        // Either validator alone is enough; sending both is what the spec
        // prefers. Re-sent on each hop, since the feed is the same feed.
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = last_modified {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
        }

        let response = request.send().await.map_err(|err| {
            Failure::new(trouble_for(&err), format!("requesting {target}: {err}"))
        })?;
        let status = response.status();

        if !status.is_redirection() {
            break (response, status);
        }
        let Some(location) = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(|location| resolve(location, &target))
        else {
            // A redirect with nowhere to go is the server's mistake, and
            // there is nothing useful to do about it.
            break (response, status);
        };

        if hops == MAX_REDIRECTS {
            return Err(Failure::new(
                Trouble::Unreachable,
                format!("{} redirected more than {MAX_REDIRECTS} times", source.url),
            ));
        }
        // Permanent only while it has been permanent all the way down.
        permanently_moved = (!moved || permanently_moved)
            && matches!(
                status,
                reqwest::StatusCode::MOVED_PERMANENTLY | reqwest::StatusCode::PERMANENT_REDIRECT
            );
        moved = true;
        target = location;
        hops += 1;
    };
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Outcome::NotModified);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
    {
        let asked_for = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);

        match (status, asked_for) {
            // A server that named a time meant it, whatever the code.
            (_, Some(retry_after)) => return Ok(Outcome::RateLimited { retry_after }),
            // "Too many requests" means it even without a number: coming
            // straight back is the thing it just asked us not to do.
            (reqwest::StatusCode::TOO_MANY_REQUESTS, None) => {
                return Ok(Outcome::RateLimited {
                    retry_after: DEFAULT_BACKOFF,
                });
            }
            // A bare 503 is usually a server restarting, not a rebuke. It
            // used to cost five minutes; it is worth one more attempt.
            _ => {}
        }
    }

    if let Err(err) = response.error_for_status_ref() {
        return Err(Failure::new(
            trouble_for_status(status),
            format!("{} answered {status}: {err}", source.url),
        ));
    }

    let header = |name: reqwest::header::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let etag = header(reqwest::header::ETAG);
    let last_modified = header(reqwest::header::LAST_MODIFIED);

    let body = body_within(response, limits.max_body, &source.url).await?;

    let not_a_feed = match parse(&body, source) {
        Ok(feed) => {
            return Ok(Outcome::Updated {
                feed: Box::new(feed),
                etag,
                last_modified,
                // A permanent redirect is the server asking to be asked
                // elsewhere. A temporary one is not, and is not remembered.
                found_at: permanently_moved.then(|| target.clone()),
            });
        }
        Err(err) => err,
    };

    // Not a feed — but a page that points at one is the commonest reason
    // somebody typed this address, so look before giving up.
    let Some(found) = may_discover
        .then(|| {
            String::from_utf8_lossy(&body)
                .split("</head")
                .next()
                .and_then(|head| discover(head, &source.url))
        })
        .flatten()
        .filter(|found| *found != source.url)
    else {
        return Err(Failure::new(Trouble::NotAFeed, format!("{not_a_feed:#}")));
    };

    // One hop, never two: a page that points at itself, or at another page,
    // must not become a chain of requests.
    let hop = FeedSource {
        url: found.clone(),
        ..source.clone()
    };
    match Box::pin(fetch_inner(client, &hop, None, None, limits, false)).await? {
        Outcome::Updated {
            mut feed,
            etag,
            last_modified,
            ..
        } => {
            // Identity stays with the address in the config. Where the feed
            // was found is where to fetch it, not what to call it — store it
            // under the discovered URL and the next prune deletes it as a
            // feed nobody subscribed to.
            feed.url = source.url.clone();
            Ok(Outcome::Updated {
                feed,
                etag,
                last_modified,
                found_at: Some(found),
            })
        }
        // A discovered feed that is rate limited or unchanged is not something
        // this hop can usefully report, so say what actually went wrong.
        _ => Err(Failure::new(
            Trouble::NotAFeed,
            format!(
                "{} points at {found}, which did not answer with a feed",
                source.url
            ),
        )),
    }
}

/// What kind of trouble a transport error is.
fn trouble_for(err: &reqwest::Error) -> Trouble {
    if err.is_timeout() {
        Trouble::TimedOut
    } else {
        // Connect, DNS, TLS and a body that stopped arriving all mean the same
        // thing to a reader: the other end is not talking to us right now.
        Trouble::Unreachable
    }
}

/// What kind of trouble a status code is.
fn trouble_for_status(status: reqwest::StatusCode) -> Trouble {
    match status.as_u16() {
        404 | 410 => Trouble::Gone,
        code if status.is_server_error() => Trouble::ServerError(code),
        code => Trouble::Refused(code),
    }
}

/// How long to wait when a server says to back off but not for how long.
const DEFAULT_BACKOFF: Duration = Duration::from_secs(300);

/// Reads a `Retry-After` value, in either of the two forms the spec allows.
///
/// Takes `now` from the caller so the HTTP-date branch is testable.
fn parse_retry_after(value: &str) -> Option<Duration> {
    retry_after_from(value, Utc::now())
}

fn retry_after_from(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let value = value.trim();
    // The common form: a number of seconds.
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds.min(MAX_BACKOFF_SECS)));
    }
    // The other form: an HTTP-date.
    let when = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let seconds = (when - now).num_seconds();
    if seconds <= 0 {
        // A date in the past means we may retry now.
        return Some(Duration::ZERO);
    }
    Some(Duration::from_secs((seconds as u64).min(MAX_BACKOFF_SECS)))
}

/// A server asking for a month off is not something to honour literally.
const MAX_BACKOFF_SECS: u64 = 24 * 60 * 60;

/// Finds the feed a web page points at, if it points at one.
///
/// Someone pasting `https://example.com/` has said what they want to read; the
/// page says where it lives, in a `<link rel="alternate">`. Failing on "that is
/// not a feed" when the answer is in the bytes we already have is unhelpful.
///
/// Atom is preferred over RSS when a page offers both — not because it is
/// better, but because a page that offers both usually treats Atom as the
/// fuller one. Comment feeds are skipped: they are a different thing, and
/// picking one would be worse than finding nothing.
pub fn discover(html: &str, base: &str) -> Option<String> {
    let mut best: Option<(u8, String)> = None;

    for tag in html.split('<').filter(|tag| {
        let name: String = tag.chars().take(4).collect::<String>().to_ascii_lowercase();
        name.starts_with("link")
    }) {
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let lower = tag.to_ascii_lowercase();
        if !lower.contains("alternate") {
            continue;
        }
        let rank = match () {
            _ if lower.contains("application/atom+xml") => 0u8,
            _ if lower.contains("application/rss+xml") => 1,
            _ if lower.contains("application/feed+json") => 2,
            _ => continue,
        };
        // A comments feed is a feed, and never the one that was meant.
        if lower.contains("comment") {
            continue;
        }
        let Some(href) = attribute(tag, "href") else {
            continue;
        };
        if best.as_ref().is_none_or(|(seen, _)| rank < *seen) {
            best = Some((rank, resolve(&href, base)));
        }
    }
    best.map(|(_, href)| href)
}

/// Reads one attribute out of a tag, in any of the three quoting styles.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(at) = lower[from..].find(name) {
        let start = from + at;
        // Must be a whole attribute name, not the tail of another one.
        let before_ok = start == 0
            || lower[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        let rest = lower[start + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let value = tag[start + name.len()..].trim_start();
            let value = value.strip_prefix('=')?.trim_start();
            let (quote, value) = match value.chars().next()? {
                q @ ('"' | '\'') => (Some(q), &value[1..]),
                _ => (None, value),
            };
            let end = match quote {
                Some(q) => value.find(q)?,
                None => value
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(value.len()),
            };
            return Some(decode_entities(&value[..end]));
        }
        from = start + name.len();
    }
    None
}

/// Resolves a possibly-relative href against the page it came from.
///
/// Enough of a URL join for the shapes feeds use: absolute, protocol-relative,
/// root-relative, and relative to the directory. Not a general implementation,
/// and it says so rather than pretending.
fn resolve(href: &str, base: &str) -> String {
    let href = href.trim();
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let scheme = if base.starts_with("http://") {
        "http:"
    } else {
        "https:"
    };
    if let Some(rest) = href.strip_prefix("//") {
        return format!("{scheme}//{rest}");
    }

    let after_scheme = base.split_once("://").map(|(_, rest)| rest).unwrap_or(base);
    let (host, path) = after_scheme.split_once('/').unwrap_or((after_scheme, ""));
    if let Some(rest) = href.strip_prefix('/') {
        return format!("{scheme}//{host}/{rest}");
    }
    // Relative to the directory the page is in.
    let directory = match path.rfind('/') {
        Some(at) => &path[..at],
        None => "",
    };
    match directory.is_empty() {
        true => format!("{scheme}//{host}/{href}"),
        false => format!("{scheme}//{host}/{directory}/{href}"),
    }
}

/// Parses raw feed bytes. Split out from [`fetch`] so it is testable offline.
pub fn parse(body: &[u8], source: &FeedSource) -> Result<Feed> {
    let parsed =
        feed_rs::parser::parse(body).with_context(|| format!("parsing feed at {}", source.url))?;

    // Decoded, like the body is. A feed that titles a post "Tom &amp; Jerry"
    // means an ampersand, and the list is the one place it was shown raw.
    // A configured title is the reader's own text, so it is left alone.
    // Blank counts as absent. danluu.com publishes `<title></title>`, and a
    // fallback that only fires on a missing element leaves a nameless line in
    // the sidebar with an unread count beside it.
    let title = source
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| {
            parsed
                .title
                .as_ref()
                .map(|t| decode_entities(&t.content))
                .filter(|title| !title.trim().is_empty())
        })
        .unwrap_or_else(|| source.url.clone());

    let mut entries: Vec<Entry> = parsed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry
                .title
                .map(|t| decode_entities(&t.content))
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "(untitled)".into());
            let link = entry.links.into_iter().next().map(|l| l.href);
            let published = entry.published.or(entry.updated);
            // Prefer the full content over the summary: it is what the article
            // renderer has to work with, and a feed that publishes both means
            // the summary to be the teaser.
            let raw = entry
                .content
                .and_then(|content| content.body)
                .or_else(|| entry.summary.map(|summary| summary.content))
                .unwrap_or_default();
            Entry {
                keys: entry_keys(&entry.id, link.as_deref(), &title, published),
                title,
                link,
                published,
                summary: to_plain_text(&raw),
                content: raw,
            }
        })
        .collect();

    // Newest first; undated entries sink to the bottom.
    entries.sort_by_key(|entry| Reverse(entry.published));

    Ok(Feed {
        title,
        url: source.url.clone(),
        entries,
        status: Status::Idle,
    })
}

/// Every identifier an entry could be recognised by, most stable first.
///
/// Prefixed by kind so a guid that happens to equal another entry's URL cannot
/// collide. The title-and-date fallback is last because it is the weakest: it
/// changes if the publisher fixes a typo. Nothing is hashed — these are written
/// to disk, and `DefaultHasher` is explicitly not stable across Rust releases.
fn entry_keys(
    id: &str,
    link: Option<&str>,
    title: &str,
    published: Option<DateTime<Utc>>,
) -> Vec<String> {
    let mut keys = Vec::with_capacity(3);
    if !id.trim().is_empty() {
        keys.push(format!("id:{id}"));
    }
    if let Some(link) = link.filter(|l| !l.trim().is_empty()) {
        keys.push(format!("link:{link}"));
    }
    let stamp = published.map(|p| p.to_rfc3339()).unwrap_or_default();
    keys.push(format!("title:{title}|{stamp}"));
    keys
}

/// Drops tags, decodes entities, and collapses whitespace so summaries fit a
/// terminal paragraph.
pub fn to_plain_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Decode before collapsing, so a decoded `&nbsp;` folds into the run of
    // whitespace around it instead of surviving as a stray character.
    let decoded = decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The named entities worth carrying, beyond the five XML defines.
///
/// Feeds are written by people typing into web editors, so they are full of
/// HTML entities that XML has never heard of. This is the short tail that
/// actually shows up; anything else is left alone.
const NAMED_ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", " "),
    ("ndash", "–"),
    ("mdash", "—"),
    ("hellip", "…"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("ldquo", "\u{201C}"),
    ("rdquo", "\u{201D}"),
    ("middot", "·"),
    ("bull", "•"),
    ("copy", "©"),
    ("reg", "®"),
    ("trade", "™"),
    ("deg", "°"),
    ("laquo", "«"),
    ("raquo", "»"),
];

/// Replaces character references with the characters they stand for.
///
/// Anything unrecognised is left exactly as it was: a summary that genuinely
/// discusses `&foo;` should still say so, and silently dropping text because we
/// did not recognise it is worse than showing it raw.
pub fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];

        // A reference is short; anything longer is an unescaped ampersand.
        let Some(end) = after.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let body = &after[..end];

        let decoded = if let Some(digits) = body.strip_prefix("#x").or(body.strip_prefix("#X")) {
            u32::from_str_radix(digits, 16)
                .ok()
                .and_then(char::from_u32)
                .map(String::from)
        } else if let Some(digits) = body.strip_prefix('#') {
            digits
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(String::from)
        } else {
            NAMED_ENTITIES
                .iter()
                .find(|(name, _)| *name == body)
                .map(|(_, value)| (*value).to_string())
        };

        match decoded {
            Some(text) => {
                out.push_str(&text);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &[u8] = br#"<?xml version="1.0"?>
        <rss version="2.0"><channel>
          <title>Example Feed</title>
          <item>
            <title>Older</title>
            <link>https://example.com/1</link>
            <pubDate>Tue, 01 Jan 2019 00:00:00 GMT</pubDate>
            <description>&lt;p&gt;First   post&lt;/p&gt;</description>
          </item>
          <item>
            <title>Newer</title>
            <link>https://example.com/2</link>
            <pubDate>Wed, 01 Jan 2020 00:00:00 GMT</pubDate>
          </item>
        </channel></rss>"#;

    fn source() -> FeedSource {
        FeedSource {
            url: "https://example.com/feed.xml".into(),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn named_entities_are_decoded() {
        assert_eq!(decode_entities("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(decode_entities("a&nbsp;b"), "a b");
        assert_eq!(
            decode_entities("&ldquo;quoted&rdquo;"),
            "\u{201C}quoted\u{201D}"
        );
    }

    #[test]
    fn numeric_entities_are_decoded_in_both_bases() {
        assert_eq!(decode_entities("it&#8217;s"), "it\u{2019}s");
        assert_eq!(decode_entities("it&#x2019;s"), "it\u{2019}s");
    }

    #[test]
    fn an_unrecognised_entity_is_left_alone_rather_than_dropped() {
        assert_eq!(decode_entities("look at &foo; here"), "look at &foo; here");
        assert_eq!(decode_entities("&#xZZZZ;"), "&#xZZZZ;");
    }

    #[test]
    fn a_bare_ampersand_survives() {
        assert_eq!(decode_entities("R&D and Q&A"), "R&D and Q&A");
        assert_eq!(decode_entities("trailing &"), "trailing &");
    }

    #[test]
    fn adjacent_entities_all_decode() {
        assert_eq!(decode_entities("&lt;&gt;&amp;"), "<>&");
    }

    #[test]
    fn summaries_have_their_entities_decoded_and_whitespace_collapsed() {
        assert_eq!(
            to_plain_text("<p>Tom &amp; Jerry&nbsp;&nbsp; say   it&#8217;s fine</p>"),
            "Tom & Jerry say it\u{2019}s fine"
        );
    }

    #[test]
    fn retry_after_reads_a_plain_number_of_seconds() {
        assert_eq!(
            retry_after_from("120", Utc::now()),
            Some(Duration::from_secs(120))
        );
    }

    #[test]
    fn retry_after_reads_an_http_date() {
        let now = DateTime::parse_from_rfc2822("Wed, 01 Jan 2020 00:00:00 GMT")
            .expect("fixture parses")
            .with_timezone(&Utc);
        assert_eq!(
            retry_after_from("Wed, 01 Jan 2020 00:01:00 GMT", now),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn a_retry_after_date_in_the_past_means_retry_now() {
        let now = DateTime::parse_from_rfc2822("Wed, 01 Jan 2020 00:10:00 GMT")
            .expect("fixture parses")
            .with_timezone(&Utc);
        assert_eq!(
            retry_after_from("Wed, 01 Jan 2020 00:00:00 GMT", now),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn an_absurd_backoff_is_capped() {
        assert_eq!(
            retry_after_from("999999999", Utc::now()),
            Some(Duration::from_secs(MAX_BACKOFF_SECS))
        );
    }

    #[test]
    fn nonsense_in_retry_after_is_ignored() {
        assert_eq!(retry_after_from("soon please", Utc::now()), None);
        assert_eq!(retry_after_from("", Utc::now()), None);
    }

    #[test]
    fn a_pending_feed_keeps_the_configured_title_and_is_marked_loading() {
        let mut source = source();
        source.title = Some("My Feed".into());
        let feed = Feed::pending(&source);
        assert_eq!(feed.title, "My Feed");
        assert_eq!(feed.status, Status::Fetching);
        assert!(feed.entries.is_empty());
    }

    #[test]
    fn a_pending_feed_without_a_title_falls_back_to_its_url() {
        assert_eq!(
            Feed::pending(&source()).title,
            "https://example.com/feed.xml"
        );
    }

    #[test]
    fn a_parsed_feed_is_idle() {
        assert_eq!(
            parse(RSS, &source()).expect("should parse").status,
            Status::Idle
        );
    }

    #[test]
    fn markers_distinguish_fetching_from_failed() {
        assert_eq!(Status::Idle.marker(), None);
        assert_eq!(Status::Fetching.marker(), Some("…"));
        assert_eq!(
            Status::Failed {
                trouble: Trouble::Unreachable,
                message: "boom".into()
            }
            .marker(),
            Some("!")
        );
    }

    #[test]
    fn only_a_failed_status_carries_an_error() {
        assert_eq!(
            Status::Failed {
                trouble: Trouble::Unreachable,
                message: "boom".into()
            }
            .error(),
            Some("boom")
        );
        assert!(Status::Idle.error().is_none());
        assert!(Status::Fetching.error().is_none());
    }

    #[test]
    fn uses_the_feed_title_when_none_is_configured() {
        let feed = parse(RSS, &source()).expect("feed should parse");
        assert_eq!(feed.title, "Example Feed");
    }

    #[test]
    fn a_configured_title_wins() {
        let mut source = source();
        source.title = Some("My Override".into());
        let feed = parse(RSS, &source).expect("feed should parse");
        assert_eq!(feed.title, "My Override");
    }

    #[test]
    fn entries_are_sorted_newest_first() {
        let feed = parse(RSS, &source()).expect("feed should parse");
        let titles: Vec<_> = feed.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Newer", "Older"]);
    }

    #[test]
    fn summaries_lose_their_markup() {
        let feed = parse(RSS, &source()).expect("feed should parse");
        assert_eq!(feed.entries[1].summary, "First post");
    }

    #[test]
    fn entries_carry_id_link_and_title_keys() {
        let feed = parse(RSS, &source()).expect("feed should parse");
        let newer = &feed.entries[0];
        assert!(newer.keys.iter().any(|k| k.starts_with("id:")));
        assert!(
            newer
                .keys
                .contains(&"link:https://example.com/2".to_string())
        );
        assert!(newer.keys.iter().any(|k| k.starts_with("title:Newer|")));
    }

    #[test]
    fn key_kinds_are_prefixed_so_they_cannot_collide() {
        // A guid equal to another entry's URL must not make them the same entry.
        let keys = entry_keys(
            "https://example.com/x",
            Some("https://example.com/x"),
            "t",
            None,
        );
        assert_eq!(keys[0], "id:https://example.com/x");
        assert_eq!(keys[1], "link:https://example.com/x");
    }

    #[test]
    fn an_entry_with_no_id_or_link_still_gets_a_key() {
        let keys = entry_keys("  ", None, "Only a title", None);
        assert_eq!(keys, vec!["title:Only a title|".to_string()]);
    }

    #[test]
    fn undated_entries_render_a_placeholder() {
        let entry = Entry {
            title: "x".into(),
            link: None,
            published: None,
            summary: String::new(),
            content: String::new(),
            keys: Vec::new(),
        };
        assert_eq!(entry.date_label(), "—");
    }

    #[test]
    fn a_title_is_decoded_the_same_way_a_summary_is() {
        // The same bytes in both places. They used to render two ways: the
        // summary decoded, the headline above it raw.
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom">
            <title>Tom &amp;amp; Jerry</title><id>u</id>
            <entry><id>e1</id>
              <title>R&amp;amp;D, Q&amp;amp;A and &amp;lt;tags&amp;gt;</title>
              <summary>R&amp;amp;D, Q&amp;amp;A and &amp;lt;tags&amp;gt;</summary>
            </entry></feed>"#;
        let feed = parse(xml.as_bytes(), &source()).expect("parses");
        let entry = &feed.entries[0];

        assert_eq!(entry.title, "R&D, Q&A and <tags>");
        assert_eq!(
            entry.title, entry.summary,
            "a title and a summary carrying the same bytes must read the same"
        );
        assert_eq!(feed.title, "Tom & Jerry", "the feed's own title too");
    }

    #[test]
    fn a_numeric_entity_in_a_title_is_decoded() {
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom">
            <title>F</title><id>u</id>
            <entry><id>e1</id><title>it&amp;#8217;s here</title></entry></feed>"#;
        let feed = parse(xml.as_bytes(), &source()).expect("parses");
        assert_eq!(feed.entries[0].title, "it\u{2019}s here");
    }

    #[test]
    fn a_configured_title_is_the_readers_own_text_and_is_left_alone() {
        // Not a feed's markup: whatever they typed in the config is literal.
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom">
            <title>Ignored</title><id>u</id></feed>"#;
        let mut source = source();
        source.title = Some("R&amp;D".into());
        let feed = parse(xml.as_bytes(), &source).expect("parses");
        assert_eq!(feed.title, "R&amp;D");
    }

    /// Serves one hand-written HTTP response on loopback and returns its port.
    ///
    /// Not "hitting the network": nothing leaves the machine, the bytes are
    /// written by the test, and the server is gone when it ends. It is the only
    /// way to test what `fetch` does with a response, as against what `parse`
    /// does with a document.
    fn serve_once(headers: &str, body: Vec<u8>) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let headers = headers.to_string();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut scratch = [0u8; 2048];
            let _ = stream.read(&mut scratch);
            let _ = stream.write_all(headers.as_bytes());
            // Ignored: the client hanging up mid-body is the point of some of
            // these tests, and a broken pipe here is that happening.
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        });
        port
    }

    fn at(port: u16) -> FeedSource {
        FeedSource {
            url: format!("http://127.0.0.1:{port}/feed.xml"),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        }
    }

    fn tiny_feed() -> Vec<u8> {
        concat!(
            r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom">"#,
            "<title>Small</title><id>u</id>",
            "<entry><id>e</id><title>One</title></entry></feed>"
        )
        .as_bytes()
        .to_vec()
    }

    #[tokio::test]
    async fn a_body_over_the_limit_is_refused_rather_than_swallowed() {
        let body = vec![b'x'; 64 * 1024];
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/atom+xml\r\nConnection: close\r\n\r\n",
            body,
        );
        let client = reqwest::Client::new();
        let limits = Limits { max_body: 8 * 1024 };

        let err = fetch(&client, &at(port), None, None, limits)
            .await
            .expect_err("a body four times the limit must not be accepted");
        let said = format!("{err:#}");
        assert!(said.contains("8192"), "the error hides the limit: {said}");
    }

    #[tokio::test]
    async fn a_declared_length_over_the_limit_is_refused_before_the_body() {
        // The body is never written: the refusal has to come from the header,
        // or a reader would download a gigabyte to learn it was a gigabyte.
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/atom+xml\r\nContent-Length: 999999999\r\nConnection: close\r\n\r\n",
            Vec::new(),
        );
        let client = reqwest::Client::new();
        let limits = Limits { max_body: 1024 };

        let err = fetch(&client, &at(port), None, None, limits)
            .await
            .expect_err("a declared length far over the limit must be refused");
        let said = format!("{err:#}");
        assert!(
            said.contains("999999999"),
            "the error does not say what was declared: {said}"
        );
    }

    #[tokio::test]
    async fn a_feed_inside_the_limit_is_read_as_usual() {
        // The limit must not be a new way for ordinary feeds to fail.
        let body = tiny_feed();
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/atom+xml\r\nConnection: close\r\n\r\n",
            body,
        );
        let client = reqwest::Client::new();

        let outcome = fetch(&client, &at(port), None, None, Limits::default())
            .await
            .expect("a small feed is fine");
        match outcome {
            Outcome::Updated { feed, .. } => {
                assert_eq!(feed.title, "Small");
                assert_eq!(feed.entries.len(), 1);
            }
            other => panic!("expected an update, got {other:?}"),
        }
    }

    #[test]
    fn the_configured_limit_is_honoured_and_zero_means_the_default() {
        use crate::config::Config;
        let mut config = Config::default();
        assert_eq!(config.limits().max_body, DEFAULT_MAX_BODY);

        config.max_feed_megabytes = Some(3);
        assert_eq!(config.limits().max_body, 3 * 1024 * 1024);

        // Zero would be a limit no feed could meet, so it reads as "unset".
        config.max_feed_megabytes = Some(0);
        assert_eq!(config.limits().max_body, DEFAULT_MAX_BODY);
    }

    /// The status codes a feed reader actually meets, and what each one means.
    #[tokio::test]
    async fn each_status_maps_to_the_kind_that_decides_what_to_do() {
        let cases = [
            (404, Trouble::Gone, false),
            (410, Trouble::Gone, false),
            (403, Trouble::Refused(403), false),
            (401, Trouble::Refused(401), false),
            (418, Trouble::Refused(418), false),
            (500, Trouble::ServerError(500), true),
            (502, Trouble::ServerError(502), true),
        ];
        let client = reqwest::Client::new();

        for (code, expected, transient) in cases {
            let port = serve_once(
                &format!("HTTP/1.1 {code} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
                Vec::new(),
            );
            let failure = fetch(&client, &at(port), None, None, Limits::default())
                .await
                .expect_err("an error status is a failure");
            assert_eq!(failure.trouble, expected, "{code} mapped wrong");
            assert_eq!(
                failure.trouble.transient(),
                transient,
                "{code} would be retried wrongly"
            );
        }
    }

    #[tokio::test]
    async fn bytes_that_are_not_a_feed_are_their_own_kind_of_trouble() {
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/atom+xml\r\nConnection: close\r\n\r\n",
            b"<html><body>Not a feed at all.</body></html>".to_vec(),
        );
        let failure = fetch(
            &reqwest::Client::new(),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect_err("html is not a feed");

        assert_eq!(failure.trouble, Trouble::NotAFeed);
        assert!(
            !failure.trouble.transient(),
            "retrying will not turn a web page into a feed"
        );
    }

    #[tokio::test]
    async fn a_host_that_is_not_listening_is_unreachable_and_worth_retrying() {
        // Port 1 on loopback: nothing is there, and the refusal is immediate.
        let source = FeedSource {
            url: "http://127.0.0.1:1/feed.xml".into(),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        };
        let failure = fetch(
            &reqwest::Client::new(),
            &source,
            None,
            None,
            Limits::default(),
        )
        .await
        .expect_err("nothing is listening");

        assert_eq!(failure.trouble, Trouble::Unreachable);
        assert!(failure.trouble.transient(), "a network comes back");
    }

    #[tokio::test]
    async fn too_much_body_is_not_worth_retrying() {
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/atom+xml\r\nConnection: close\r\n\r\n",
            vec![b'x'; 64 * 1024],
        );
        let failure = fetch(
            &reqwest::Client::new(),
            &at(port),
            None,
            None,
            Limits { max_body: 8 * 1024 },
        )
        .await
        .expect_err("over the limit");

        assert_eq!(failure.trouble, Trouble::TooBig);
        assert!(
            !failure.trouble.transient(),
            "asking again gets the same oversized body"
        );
    }

    #[test]
    fn every_kind_reads_as_a_sentence_rather_than_a_variant_name() {
        for trouble in [
            Trouble::Unreachable,
            Trouble::TimedOut,
            Trouble::ServerError(503),
            Trouble::Refused(403),
            Trouble::Gone,
            Trouble::TooBig,
            Trouble::NotAFeed,
        ] {
            let said = trouble.sentence();
            assert!(
                said.chars().next().is_some_and(|c| c.is_lowercase()),
                "{said:?} does not continue a sentence beginning with the feed name"
            );
            assert!(!said.contains("Trouble"), "{said:?} leaks the type name");
        }
    }

    #[test]
    fn a_failed_status_carries_both_the_kind_and_the_words() {
        let status = Status::Failed {
            trouble: Trouble::Gone,
            message: "Old Blog is no longer there".into(),
        };
        assert_eq!(status.trouble(), Some(Trouble::Gone));
        assert_eq!(status.error(), Some("Old Blog is no longer there"));
        assert_eq!(Status::Idle.trouble(), None);
    }

    #[test]
    fn retries_stop_after_the_allowed_number() {
        let retry = Retry::default();
        let url = "https://example.com/feed.xml";
        assert!(retry.delay(1, url, Duration::ZERO).is_some());
        assert!(retry.delay(2, url, Duration::ZERO).is_some());
        assert!(
            retry.delay(3, url, Duration::ZERO).is_none(),
            "a third retry is one more than the two allowed"
        );
    }

    #[test]
    fn zero_attempts_never_retries() {
        let retry = Retry {
            attempts: 0,
            ..Retry::default()
        };
        assert!(
            retry
                .delay(1, "https://example.com/x", Duration::ZERO)
                .is_none()
        );
    }

    #[test]
    fn each_wait_is_longer_than_the_one_before() {
        let retry = Retry {
            attempts: 5,
            first_delay: Duration::from_millis(400),
            max_delay: Duration::from_secs(30),
            max_total: Duration::from_secs(300),
        };
        let url = "https://example.com/feed.xml";
        let first = retry.delay(1, url, Duration::ZERO).expect("a first wait");
        let second = retry.delay(2, url, Duration::ZERO).expect("a second wait");
        let third = retry.delay(3, url, Duration::ZERO).expect("a third wait");
        assert!(second > first, "{second:?} is not longer than {first:?}");
        assert!(third > second, "{third:?} is not longer than {second:?}");
    }

    #[test]
    fn no_single_wait_exceeds_the_ceiling() {
        let retry = Retry {
            attempts: 20,
            first_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(4),
            max_total: Duration::from_secs(600),
        };
        for attempt in 1..=20 {
            let wait = retry
                .delay(attempt, "https://example.com/feed.xml", Duration::ZERO)
                .expect("a wait");
            // The ceiling plus the spread it allows, which is half again.
            assert!(
                wait <= Duration::from_secs(6),
                "attempt {attempt} would wait {wait:?}"
            );
        }
    }

    #[test]
    fn the_total_budget_ends_it_however_many_attempts_are_left() {
        let retry = Retry {
            attempts: 10,
            max_total: Duration::from_secs(10),
            ..Retry::default()
        };
        let url = "https://example.com/feed.xml";
        assert!(retry.delay(1, url, Duration::from_secs(3)).is_some());
        assert!(
            retry.delay(1, url, Duration::from_secs(10)).is_none(),
            "the budget is spent, so there is nothing left to wait with"
        );
    }

    #[test]
    fn a_wait_never_runs_past_what_is_left_of_the_budget() {
        let retry = Retry {
            attempts: 10,
            first_delay: Duration::from_secs(9),
            max_delay: Duration::from_secs(60),
            max_total: Duration::from_secs(10),
        };
        let wait = retry
            .delay(1, "https://example.com/feed.xml", Duration::from_secs(9))
            .expect("a wait");
        assert!(
            wait <= Duration::from_secs(1),
            "{wait:?} would sleep through the budget and then give up anyway"
        );
    }

    #[test]
    fn feeds_on_one_host_do_not_come_back_in_lockstep() {
        // The case this exists for: fifteen feeds on one site, all failing at
        // once because the site is down, all retrying at the same instant.
        let retry = Retry::default();
        let waits: Vec<Duration> = (0..15)
            .map(|n| {
                retry
                    .delay(
                        1,
                        &format!("https://one-host.example/feed{n}.xml"),
                        Duration::ZERO,
                    )
                    .expect("a wait")
            })
            .collect();

        let distinct: std::collections::HashSet<u128> =
            waits.iter().map(|wait| wait.as_millis()).collect();
        assert!(
            distinct.len() >= 10,
            "fifteen feeds produced only {} different waits: {waits:?}",
            distinct.len()
        );
    }

    #[test]
    fn the_same_feed_always_waits_the_same() {
        // Deterministic, so a failure reproduces and a test can assert on it.
        let retry = Retry::default();
        let url = "https://example.com/feed.xml";
        assert_eq!(
            retry.delay(1, url, Duration::ZERO),
            retry.delay(1, url, Duration::ZERO)
        );
    }

    #[test]
    fn only_what_could_succeed_next_time_is_retried() {
        // The pairing that makes retrying safe: the taxonomy decides.
        for trouble in [
            Trouble::Unreachable,
            Trouble::TimedOut,
            Trouble::ServerError(503),
        ] {
            assert!(trouble.transient(), "{trouble:?} should be retried");
        }
        for trouble in [
            Trouble::Gone,
            Trouble::Refused(403),
            Trouble::TooBig,
            Trouble::NotAFeed,
        ] {
            assert!(!trouble.transient(), "{trouble:?} must not be retried");
        }
    }

    #[test]
    fn the_configured_attempts_are_honoured_and_capped() {
        use crate::config::Config;
        let mut config = Config::default();
        assert_eq!(config.retry().attempts, Retry::default().attempts);

        config.retry_attempts = Some(0);
        assert_eq!(
            config.retry().attempts,
            0,
            "zero must mean zero, not default"
        );

        config.retry_attempts = Some(4);
        assert_eq!(config.retry().attempts, 4);

        config.retry_attempts = Some(500);
        assert!(
            config.retry().attempts <= 5,
            "a silly number would wait minutes on a dead feed"
        );
    }

    #[tokio::test]
    async fn a_bare_503_is_retried_rather_than_costing_five_minutes() {
        // Service Unavailable with no Retry-After is usually a server coming
        // back up, not a rebuke. Treating it as rate limiting used to park the
        // feed for the default backoff.
        let port = serve_once(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            Vec::new(),
        );
        let failure = fetch(
            &reqwest::Client::new(),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect_err("a 503 with no instructions is a failure, not a deferral");

        assert_eq!(failure.trouble, Trouble::ServerError(503));
        assert!(failure.trouble.transient(), "so it will be tried again");
    }

    #[tokio::test]
    async fn a_503_that_names_a_time_is_still_honoured() {
        let port = serve_once(
            "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            Vec::new(),
        );
        let outcome = fetch(
            &reqwest::Client::new(),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect("a named wait is an outcome, not an error");

        match outcome {
            Outcome::RateLimited { retry_after } => {
                assert_eq!(retry_after, Duration::from_secs(120));
            }
            other => panic!("expected a deferral, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn too_many_requests_backs_off_even_without_a_number() {
        // 429 means it whether or not it says for how long.
        let port = serve_once(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            Vec::new(),
        );
        let outcome = fetch(
            &reqwest::Client::new(),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect("rate limiting is an outcome");

        match outcome {
            Outcome::RateLimited { retry_after } => assert_eq!(retry_after, DEFAULT_BACKOFF),
            other => panic!("expected a deferral, got {other:?}"),
        }
    }

    #[test]
    fn a_blank_title_falls_back_the_way_a_missing_one_does() {
        // The shape danluu.com actually publishes: the element is there and
        // empty, so a fallback that tests for absence never fires.
        let xml = r#"<?xml version="1.0"?>
            <rss version="2.0"><channel>
              <title></title>
              <link>https://example.com/</link>
              <description>Recent content on </description>
              <item><title>   </title><link>https://example.com/a</link></item>
            </channel></rss>"#;
        let feed = parse(xml.as_bytes(), &source()).expect("parses");

        assert_eq!(
            feed.title,
            source().url,
            "an empty feed title left a nameless line in the sidebar"
        );
        assert_eq!(
            feed.entries[0].title, "(untitled)",
            "whitespace is not a title"
        );
    }

    #[test]
    fn a_blank_configured_title_does_not_win_over_the_feeds_own() {
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom">
            <title>The Real Name</title><id>u</id></feed>"#;
        let mut source = source();
        source.title = Some("  ".into());
        assert_eq!(
            parse(xml.as_bytes(), &source).expect("parses").title,
            "The Real Name"
        );
    }

    #[test]
    fn a_page_that_names_its_feed_is_followed() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/atom+xml" href="/atom.xml" title="Feed">
            </head><body>Hello</body></html>"#;
        assert_eq!(
            discover(html, "https://example.com/").as_deref(),
            Some("https://example.com/atom.xml")
        );
    }

    #[test]
    fn atom_wins_when_a_page_offers_both() {
        let html = r#"<head>
            <link rel="alternate" type="application/rss+xml" href="/rss.xml">
            <link rel="alternate" type="application/atom+xml" href="/atom.xml">
            </head>"#;
        assert_eq!(
            discover(html, "https://example.com/").as_deref(),
            Some("https://example.com/atom.xml"),
            "a page offering both usually treats Atom as the fuller one"
        );
    }

    #[test]
    fn a_comments_feed_is_never_the_one_that_was_meant() {
        let html = r#"<head>
            <link rel="alternate" type="application/rss+xml" href="/comments/feed" title="Comments">
            <link rel="alternate" type="application/rss+xml" href="/feed">
            </head>"#;
        assert_eq!(
            discover(html, "https://example.com/").as_deref(),
            Some("https://example.com/feed")
        );
    }

    #[test]
    fn relative_hrefs_resolve_against_the_page_they_came_from() {
        let cases = [
            (
                "https://example.com/blog/index.html",
                "feed.xml",
                "https://example.com/blog/feed.xml",
            ),
            (
                "https://example.com/blog/",
                "feed.xml",
                "https://example.com/blog/feed.xml",
            ),
            (
                "https://example.com/",
                "/feed.xml",
                "https://example.com/feed.xml",
            ),
            (
                "https://example.com/deep/page",
                "/feed.xml",
                "https://example.com/feed.xml",
            ),
            (
                "https://example.com/",
                "//cdn.example/feed.xml",
                "https://cdn.example/feed.xml",
            ),
            (
                "http://example.com/",
                "//cdn.example/f.xml",
                "http://cdn.example/f.xml",
            ),
            (
                "https://example.com/",
                "https://other.example/f.xml",
                "https://other.example/f.xml",
            ),
        ];
        for (base, href, expected) in cases {
            let html =
                format!(r#"<link rel="alternate" type="application/atom+xml" href="{href}">"#);
            assert_eq!(
                discover(&html, base).as_deref(),
                Some(expected),
                "{href} against {base}"
            );
        }
    }

    #[test]
    fn every_quoting_style_a_page_might_use_is_read() {
        for tag in [
            r#"<link rel="alternate" type="application/atom+xml" href="/f.xml">"#,
            r#"<link rel='alternate' type='application/atom+xml' href='/f.xml'>"#,
            r#"<link rel=alternate type=application/atom+xml href=/f.xml>"#,
            r#"<LINK REL="ALTERNATE" TYPE="APPLICATION/ATOM+XML" HREF="/f.xml">"#,
            r#"<link href="/f.xml" type="application/atom+xml" rel="alternate">"#,
        ] {
            assert_eq!(
                discover(tag, "https://example.com/").as_deref(),
                Some("https://example.com/f.xml"),
                "could not read {tag}"
            );
        }
    }

    #[test]
    fn a_page_with_no_feed_finds_nothing_rather_than_guessing() {
        for html in [
            "<html><body>Just a page.</body></html>",
            r#"<link rel="stylesheet" href="/style.css">"#,
            r#"<link rel="alternate" hreflang="de" href="/de/">"#,
            r#"<link rel="alternate" type="text/html" href="/other">"#,
        ] {
            assert_eq!(discover(html, "https://example.com/"), None, "{html}");
        }
    }

    #[test]
    fn an_href_with_an_entity_in_it_is_decoded() {
        let html =
            r#"<link rel="alternate" type="application/atom+xml" href="/f.xml?a=1&amp;b=2">"#;
        assert_eq!(
            discover(html, "https://example.com/").as_deref(),
            Some("https://example.com/f.xml?a=1&b=2")
        );
    }

    /// Serves a page that names a feed, then the feed itself.
    fn serve_page_then_feed(page: String, feed: Vec<u8>) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        std::thread::spawn(move || {
            for (n, incoming) in listener.incoming().enumerate().take(2) {
                let Ok(mut stream) = incoming else { return };
                let mut scratch = [0u8; 2048];
                let _ = stream.read(&mut scratch);
                let (kind, body) = if n == 0 {
                    ("text/html", page.clone().into_bytes())
                } else {
                    ("application/atom+xml", feed.clone())
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        port
    }

    #[tokio::test]
    async fn a_site_address_finds_the_feed_the_page_points_at() {
        // What somebody actually types: the address of the site they read.
        let port = serve_page_then_feed(
            r#"<html><head><link rel="alternate" type="application/atom+xml" href="/atom.xml"></head></html>"#.into(),
            tiny_feed(),
        );
        let source = FeedSource {
            url: format!("http://127.0.0.1:{port}/"),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        };

        let outcome = fetch(
            &reqwest::Client::new(),
            &source,
            None,
            None,
            Limits::default(),
        )
        .await
        .expect("the page named a feed, so there is a feed");

        match outcome {
            Outcome::Updated { feed, found_at, .. } => {
                assert_eq!(feed.title, "Small");
                assert_eq!(
                    found_at.as_deref(),
                    Some(format!("http://127.0.0.1:{port}/atom.xml").as_str()),
                    "the caller was not told where it was found, so it cannot remember"
                );
            }
            other => panic!("expected a feed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_page_pointing_at_another_page_fails_rather_than_looping() {
        // One hop, never two. A page that points at a page is a chain, and a
        // chain is how a reader ends up making requests forever.
        let port = serve_page_then_feed(
            r#"<link rel="alternate" type="application/atom+xml" href="/also-a-page">"#.into(),
            b"<html><head><link rel=\"alternate\" type=\"application/atom+xml\" href=\"/third\"></head></html>".to_vec(),
        );
        let source = FeedSource {
            url: format!("http://127.0.0.1:{port}/"),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        };

        let failure = fetch(
            &reqwest::Client::new(),
            &source,
            None,
            None,
            Limits::default(),
        )
        .await
        .expect_err("two hops is a chain, not a discovery");
        assert_eq!(failure.trouble, Trouble::NotAFeed);
    }

    #[tokio::test]
    async fn a_page_with_no_feed_says_so_rather_than_only_not_a_feed() {
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n",
            b"<html><head><title>A site</title></head><body>No feed here.</body></html>".to_vec(),
        );
        let failure = fetch(
            &reqwest::Client::new(),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect_err("no feed anywhere");
        assert_eq!(failure.trouble, Trouble::NotAFeed);
    }

    #[tokio::test]
    async fn a_discovered_feed_keeps_the_address_it_was_subscribed_under() {
        // The configured URL is the identity; the discovered one is only where
        // to fetch. Storing entries under the discovered address puts them
        // beside a feed nobody subscribed to, and the next prune removes them.
        let port = serve_page_then_feed(
            r#"<link rel="alternate" type="application/atom+xml" href="/atom.xml">"#.into(),
            tiny_feed(),
        );
        let configured = format!("http://127.0.0.1:{port}/");
        let source = FeedSource {
            url: configured.clone(),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        };

        match fetch(
            &reqwest::Client::new(),
            &source,
            None,
            None,
            Limits::default(),
        )
        .await
        .expect("discovered")
        {
            Outcome::Updated { feed, found_at, .. } => {
                assert_eq!(feed.url, configured, "the feed changed its own identity");
                assert_ne!(found_at.as_deref(), Some(configured.as_str()));
            }
            other => panic!("expected a feed, got {other:?}"),
        }
    }

    /// Serves a chain: each entry is (status, location-or-body).
    fn serve_chain(steps: Vec<(u16, String)>, body: Vec<u8>) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        std::thread::spawn(move || {
            let total = steps.len() + 1;
            for (n, incoming) in listener.incoming().enumerate().take(total) {
                let Ok(mut stream) = incoming else { return };
                let mut scratch = [0u8; 2048];
                let _ = stream.read(&mut scratch);
                let out = match steps.get(n) {
                    Some((code, location)) => format!(
                        "HTTP/1.1 {code} Moved\r\nLocation: http://127.0.0.1:{port}{location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .into_bytes(),
                    None => {
                        let mut head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/atom+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        head.extend_from_slice(&body);
                        head
                    }
                };
                let _ = stream.write_all(&out);
                let _ = stream.flush();
            }
        });
        port
    }

    async fn follow(port: u16) -> Outcome {
        fetch(
            &reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("a client"),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect("the chain ends in a feed")
    }

    #[tokio::test]
    async fn a_permanent_redirect_is_remembered() {
        for code in [301u16, 308] {
            let port = serve_chain(vec![(code, "/moved.xml".into())], tiny_feed());
            match follow(port).await {
                Outcome::Updated { feed, found_at, .. } => {
                    assert_eq!(feed.title, "Small", "the redirect was not followed");
                    assert_eq!(
                        found_at.as_deref(),
                        Some(format!("http://127.0.0.1:{port}/moved.xml").as_str()),
                        "{code} was not remembered, so it will be paid again forever"
                    );
                }
                other => panic!("expected a feed, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_temporary_redirect_is_followed_but_not_remembered() {
        for code in [302u16, 303, 307] {
            let port = serve_chain(vec![(code, "/elsewhere.xml".into())], tiny_feed());
            match follow(port).await {
                Outcome::Updated { feed, found_at, .. } => {
                    assert_eq!(feed.title, "Small", "the redirect was not followed");
                    assert_eq!(found_at, None, "{code} is temporary by definition");
                }
                other => panic!("expected a feed, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_chain_that_is_permanent_all_the_way_down_is_remembered() {
        let port = serve_chain(
            vec![(301, "/one.xml".into()), (308, "/two.xml".into())],
            tiny_feed(),
        );
        match follow(port).await {
            Outcome::Updated { found_at, .. } => assert_eq!(
                found_at.as_deref(),
                Some(format!("http://127.0.0.1:{port}/two.xml").as_str())
            ),
            other => panic!("expected a feed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn one_temporary_hop_makes_the_whole_chain_temporary() {
        // The address at the end is only reliable if every step to it was.
        let port = serve_chain(
            vec![(301, "/one.xml".into()), (302, "/two.xml".into())],
            tiny_feed(),
        );
        match follow(port).await {
            Outcome::Updated { found_at, .. } => assert_eq!(
                found_at, None,
                "a chain with a temporary hop in it is not a permanent move"
            ),
            other => panic!("expected a feed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_redirect_loop_gives_up_rather_than_going_round() {
        let steps: Vec<(u16, String)> = (0..MAX_REDIRECTS + 2)
            .map(|n| (301u16, format!("/hop{n}.xml")))
            .collect();
        let port = serve_chain(steps, tiny_feed());

        let failure = fetch(
            &reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("a client"),
            &at(port),
            None,
            None,
            Limits::default(),
        )
        .await
        .expect_err("a chain this long is a loop");
        assert!(
            failure.detail.contains("redirected more than"),
            "{}",
            failure.detail
        );
    }
}
