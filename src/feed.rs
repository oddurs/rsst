use std::cmp::Reverse;

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
    /// True until this feed's first fetch resolves. Never cached — a restored
    /// feed is not in flight.
    #[serde(skip)]
    pub loading: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub title: String,
    pub link: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub summary: String,
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
            loading: true,
        }
    }
}

/// Downloads and parses a single feed.
pub async fn fetch(client: &reqwest::Client, source: &FeedSource) -> Result<Feed> {
    let body = client
        .get(&source.url)
        .send()
        .await
        .with_context(|| format!("requesting {}", source.url))?
        .error_for_status()
        .with_context(|| format!("bad status from {}", source.url))?
        .bytes()
        .await
        .with_context(|| format!("reading body of {}", source.url))?;

    parse(&body, source)
}

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
            Entry {
                keys: entry_keys(&entry.id, link.as_deref(), &title, published),
                title,
                link,
                published,
                summary: entry
                    .summary
                    .map(|t| strip_html(&t.content))
                    .or_else(|| entry.content.and_then(|c| c.body).map(|b| strip_html(&b)))
                    .unwrap_or_default(),
            }
        })
        .collect();

    // Newest first; undated entries sink to the bottom.
    entries.sort_by_key(|entry| Reverse(entry.published));

    Ok(Feed {
        title,
        url: source.url.clone(),
        entries,
        loading: false,
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

/// Drops tags and collapses whitespace so summaries fit a terminal paragraph.
fn strip_html(input: &str) -> String {
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
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
            title: None,
        }
    }

    #[test]
    fn a_pending_feed_keeps_the_configured_title_and_is_marked_loading() {
        let mut source = source();
        source.title = Some("My Feed".into());
        let feed = Feed::pending(&source);
        assert_eq!(feed.title, "My Feed");
        assert!(feed.loading);
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
    fn a_parsed_feed_is_not_loading() {
        assert!(!parse(RSS, &source()).expect("should parse").loading);
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
            keys: Vec::new(),
        };
        assert_eq!(entry.date_label(), "—");
    }
}
