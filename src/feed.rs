use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::config::FeedSource;

/// A fetched feed and its entries, ready for display.
#[derive(Debug, Clone)]
pub struct Feed {
    pub title: String,
    pub url: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub title: String,
    pub link: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub summary: String,
}

impl Entry {
    /// `YYYY-MM-DD`, or an em dash when the feed omitted a date.
    pub fn date_label(&self) -> String {
        self.published
            .map(|when| when.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".into())
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
        .map(|entry| Entry {
            title: entry
                .title
                .map(|t| t.content)
                .unwrap_or_else(|| "(untitled)".into()),
            link: entry.links.into_iter().next().map(|l| l.href),
            published: entry.published.or(entry.updated),
            summary: entry
                .summary
                .map(|t| strip_html(&t.content))
                .or_else(|| entry.content.and_then(|c| c.body).map(|b| strip_html(&b)))
                .unwrap_or_default(),
        })
        .collect();

    // Newest first; undated entries sink to the bottom.
    entries.sort_by(|a, b| b.published.cmp(&a.published));

    Ok(Feed {
        title,
        url: source.url.clone(),
        entries,
    })
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
    fn undated_entries_render_a_placeholder() {
        let entry = Entry {
            title: "x".into(),
            link: None,
            published: None,
            summary: String::new(),
        };
        assert_eq!(entry.date_label(), "—");
    }
}
