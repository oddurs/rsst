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

/// Downloads and parses a feed, asking the server to skip it if unchanged.
pub async fn fetch(
    client: &reqwest::Client,
    source: &FeedSource,
    etag: Option<&str>,
    last_modified: Option<&str>,
    limits: Limits,
) -> std::result::Result<Outcome, Failure> {
    let mut request = client.get(&source.url);
    // Either validator alone is enough; sending both is what the spec prefers.
    if let Some(etag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = last_modified {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
    }

    let response = request.send().await.map_err(|err| {
        Failure::new(
            trouble_for(&err),
            format!("requesting {}: {err}", source.url),
        )
    })?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Outcome::NotModified);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
    {
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after)
            // A server that says "slow down" without saying how long still
            // means it, so do not come straight back.
            .unwrap_or(DEFAULT_BACKOFF);
        return Ok(Outcome::RateLimited { retry_after });
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
    let feed =
        parse(&body, source).map_err(|err| Failure::new(Trouble::NotAFeed, format!("{err:#}")))?;

    Ok(Outcome::Updated {
        feed: Box::new(feed),
        etag,
        last_modified,
    })
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

/// Parses raw feed bytes. Split out from [`fetch`] so it is testable offline.
pub fn parse(body: &[u8], source: &FeedSource) -> Result<Feed> {
    let parsed =
        feed_rs::parser::parse(body).with_context(|| format!("parsing feed at {}", source.url))?;

    // Decoded, like the body is. A feed that titles a post "Tom &amp; Jerry"
    // means an ampersand, and the list is the one place it was shown raw.
    // A configured title is the reader's own text, so it is left alone.
    let title = source.title.clone().unwrap_or_else(|| {
        parsed
            .title
            .as_ref()
            .map(|t| decode_entities(&t.content))
            .unwrap_or_else(|| source.url.clone())
    });

    let mut entries: Vec<Entry> = parsed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry
                .title
                .map(|t| decode_entities(&t.content))
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
}
