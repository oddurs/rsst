//! OPML subscription lists — the interchange format every other reader speaks.

use anyhow::{Context, Result};
use quick_xml::escape::escape;
use quick_xml::events::Event;

use crate::config::FeedSource;

/// Reads the feeds out of an OPML document.
///
/// Folder structure is flattened: an outline is a feed if it has an `xmlUrl`,
/// and a folder otherwise. Nesting is arbitrarily deep in the wild, and until
/// rsst has somewhere to put a folder there is nothing useful to do with it.
pub fn parse(xml: &str) -> Result<Vec<FeedSource>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut feeds: Vec<FeedSource> = Vec::new();
    let mut buf = Vec::new();

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .with_context(|| format!("parsing OPML at byte {}", reader.buffer_position()))?;

        match event {
            Event::Eof => break,
            // An outline can be either form depending on whether it has children.
            Event::Start(ref e) | Event::Empty(ref e) if e.local_name().as_ref() == b"outline" => {
                let mut url = None;
                let mut title = None;
                let mut text = None;

                let decoder = reader.decoder();
                for attr in e.attributes().with_checks(false) {
                    let attr = attr.context("reading an outline attribute")?;
                    // The decoder-aware call, because feed-rs turns on
                    // quick-xml's `encoding` feature and feature unification
                    // removes the simpler `unescape_value` from the build.
                    let value = attr
                        .decoded_and_normalized_value(quick_xml::XmlVersion::default(), decoder)
                        .context("unescaping an outline attribute")?
                        .into_owned();
                    // Attribute names are case-insensitive in practice: exports
                    // use xmlUrl, xmlurl and XMLURL interchangeably.
                    match attr
                        .key
                        .local_name()
                        .as_ref()
                        .to_ascii_lowercase()
                        .as_slice()
                    {
                        b"xmlurl" => url = Some(value),
                        b"title" => title = Some(value),
                        b"text" => text = Some(value),
                        _ => {}
                    }
                }

                if let Some(url) = url.filter(|u| !u.trim().is_empty()) {
                    // `title` is the stricter attribute; `text` is what most
                    // exporters actually populate.
                    let title = title.or(text).filter(|t| !t.trim().is_empty());
                    if !feeds.iter().any(|f| f.url == url) {
                        feeds.push(FeedSource { url, title });
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(feeds)
}

/// Renders feeds as an OPML document that [`parse`] reads back identically.
pub fn write(feeds: &[FeedSource]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <opml version=\"2.0\">\n\
         \x20 <head>\n\
         \x20   <title>rsst subscriptions</title>\n\
         \x20 </head>\n\
         \x20 <body>\n",
    );
    for feed in feeds {
        let title = feed.title.clone().unwrap_or_else(|| feed.url.clone());
        out.push_str(&format!(
            "    <outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\"/>\n",
            escape(&title),
            escape(&title),
            escape(&feed.url),
        ));
    }
    out.push_str("  </body>\n</opml>\n");
    out
}

/// Adds `incoming` to `existing`, skipping URLs already present.
///
/// Returns how many were actually new, so the caller can say so.
pub fn merge(existing: &mut Vec<FeedSource>, incoming: Vec<FeedSource>) -> usize {
    let mut added = 0;
    for feed in incoming {
        if !existing.iter().any(|f| f.url == feed.url) {
            existing.push(feed);
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shaped like a real NetNewsWire/Feedly export: nested folders, mixed
    // attribute spellings, and outlines that are folders rather than feeds.
    const REAL_WORLD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
        <opml version="1.0">
          <head><title>subscriptions</title></head>
          <body>
            <outline text="Rust">
              <outline type="rss" text="Rust Blog" title="Rust Blog"
                       xmlUrl="https://blog.rust-lang.org/feed.xml"
                       htmlUrl="https://blog.rust-lang.org/"/>
              <outline type="rss" text="TWiR" xmlUrl="https://this-week-in-rust.org/atom.xml"/>
            </outline>
            <outline text="Empty Folder"/>
            <outline type="rss" text="Top Level &amp; Co" xmlurl="https://example.com/feed"/>
          </body>
        </opml>"#;

    fn source(url: &str, title: Option<&str>) -> FeedSource {
        FeedSource {
            url: url.into(),
            title: title.map(Into::into),
        }
    }

    #[test]
    fn reads_a_real_world_export() {
        let feeds = parse(REAL_WORLD).expect("should parse");
        let urls: Vec<_> = feeds.iter().map(|f| f.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://blog.rust-lang.org/feed.xml",
                "https://this-week-in-rust.org/atom.xml",
                "https://example.com/feed",
            ]
        );
    }

    #[test]
    fn folders_are_flattened_not_imported_as_feeds() {
        let feeds = parse(REAL_WORLD).expect("should parse");
        assert!(
            !feeds.iter().any(|f| f.title.as_deref() == Some("Rust")),
            "the folder outline became a feed"
        );
        assert_eq!(feeds.len(), 3);
    }

    #[test]
    fn entities_in_titles_are_decoded() {
        let feeds = parse(REAL_WORLD).expect("should parse");
        assert_eq!(feeds[2].title.as_deref(), Some("Top Level & Co"));
    }

    #[test]
    fn attribute_case_does_not_matter() {
        // The third entry uses `xmlurl`, not `xmlUrl`.
        let feeds = parse(REAL_WORLD).expect("should parse");
        assert_eq!(feeds[2].url, "https://example.com/feed");
    }

    #[test]
    fn a_round_trip_preserves_the_feed_list() {
        let original = vec![
            source("https://a.example/feed", Some("A & B")),
            source("https://b.example/feed", None),
        ];
        let reparsed = parse(&write(&original)).expect("should parse");

        assert_eq!(reparsed.len(), 2);
        assert_eq!(reparsed[0].url, original[0].url);
        assert_eq!(reparsed[0].title.as_deref(), Some("A & B"));
        // A feed with no title round-trips as its URL, which is what is shown.
        assert_eq!(reparsed[1].url, original[1].url);
    }

    #[test]
    fn importing_twice_adds_nothing_the_second_time() {
        let incoming = parse(REAL_WORLD).expect("should parse");
        let mut existing = Vec::new();

        assert_eq!(merge(&mut existing, incoming.clone()), 3);
        assert_eq!(merge(&mut existing, incoming), 0);
        assert_eq!(existing.len(), 3);
    }

    #[test]
    fn merging_keeps_feeds_that_are_already_configured() {
        let mut existing = vec![source("https://a.example/feed", Some("Mine"))];
        let added = merge(
            &mut existing,
            vec![
                source("https://a.example/feed", Some("Theirs")),
                source("https://new.example/feed", None),
            ],
        );
        assert_eq!(added, 1);
        // The local title wins; an import must not rewrite what you chose.
        assert_eq!(existing[0].title.as_deref(), Some("Mine"));
        assert_eq!(existing.len(), 2);
    }

    #[test]
    fn a_duplicate_inside_one_document_is_taken_once() {
        let xml = r#"<opml><body>
            <outline type="rss" xmlUrl="https://a.example/feed"/>
            <outline type="rss" xmlUrl="https://a.example/feed"/>
        </body></opml>"#;
        assert_eq!(parse(xml).expect("should parse").len(), 1);
    }

    #[test]
    fn an_outline_without_a_url_is_not_a_feed() {
        let xml = r#"<opml><body><outline text="just a folder"/></body></opml>"#;
        assert!(parse(xml).expect("should parse").is_empty());
    }

    #[test]
    fn malformed_xml_is_an_error_rather_than_silence() {
        assert!(parse("<opml><body><outline").is_err());
    }
}
