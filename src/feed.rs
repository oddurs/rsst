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

/// What happened, or is happening, to a feed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Status {
    /// Fetched, or restored from cache and not currently being fetched.
    #[default]
    Idle,
    /// A request is in flight.
    Fetching,
    /// The last attempt failed. Carries the message, which is the only place
    /// the reason survives.
    Failed(String),
}

impl Status {
    /// The marker shown beside the feed's name.
    pub fn marker(&self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Fetching => Some("…"),
            Self::Failed(_) => Some("!"),
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
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

/// Downloads and parses a feed, asking the server to skip it if unchanged.
pub async fn fetch(
    client: &reqwest::Client,
    source: &FeedSource,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<Outcome> {
    let mut request = client.get(&source.url);
    // Either validator alone is enough; sending both is what the spec prefers.
    if let Some(etag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = last_modified {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("requesting {}", source.url))?;

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

    let response = response
        .error_for_status()
        .with_context(|| format!("bad status from {}", source.url))?;

    let header = |name: reqwest::header::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let etag = header(reqwest::header::ETAG);
    let last_modified = header(reqwest::header::LAST_MODIFIED);

    let body = response
        .bytes()
        .await
        .with_context(|| format!("reading body of {}", source.url))?;

    Ok(Outcome::Updated {
        feed: Box::new(parse(&body, source)?),
        etag,
        last_modified,
    })
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

    let title = source
        .title
        .clone()
        .or_else(|| parsed.title.as_ref().map(|t| t.content.clone()))
        .unwrap_or_else(|| source.url.clone());

    let mut entries: Vec<Entry> = parsed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry
                .title
                .map(|t| t.content)
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
        assert_eq!(Status::Failed("boom".into()).marker(), Some("!"));
    }

    #[test]
    fn only_a_failed_status_carries_an_error() {
        assert_eq!(Status::Failed("boom".into()).error(), Some("boom"));
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
}
