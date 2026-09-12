//! OPML subscription lists — the interchange format every other reader speaks.

use anyhow::{Context, Result};
use quick_xml::escape::escape;
use quick_xml::events::Event;

use crate::config::FeedSource;

/// Reads the feeds out of an OPML document.
///
/// An outline is a feed if it has an `xmlUrl`, and a folder otherwise. Folders
/// become tags on the feeds inside them; nesting is arbitrarily deep in the
/// wild, so every enclosing folder is applied.
pub fn parse(xml: &str) -> Result<Vec<FeedSource>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut feeds: Vec<FeedSource> = Vec::new();
    let mut buf = Vec::new();
    // Enclosing folder names, outermost first.
    let mut folders: Vec<String> = Vec::new();

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .with_context(|| format!("parsing OPML at byte {}", reader.buffer_position()))?;

        match event {
            Event::Eof => break,
            // An outline can be either form depending on whether it has children.
            Event::End(ref e) if e.local_name().as_ref() == b"outline" => {
                folders.pop();
            }
            Event::Start(ref e) | Event::Empty(ref e) if e.local_name().as_ref() == b"outline" => {
                let is_start = matches!(event, Event::Start(_));
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

                // `title` is the stricter attribute; `text` is what most
                // exporters actually populate.
                let label = title.or(text).filter(|t| !t.trim().is_empty());

                match url.filter(|u| !u.trim().is_empty()) {
                    Some(url) => {
                        if !feeds.iter().any(|f| f.url == url) {
                            feeds.push(FeedSource {
                                url,
                                title: label,
                                tags: folders.clone(),
                                refresh_minutes: None,
                            });
                        }
                        // A feed written as <outline>...</outline> still closes,
                        // so keep the stack balanced with a placeholder.
                        if is_start {
                            folders.push(String::new());
                        }
                    }
                    // A folder: everything inside it gets this tag.
                    None if is_start => folders.push(label.unwrap_or_default()),
                    None => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(feeds)
}

/// Renders feeds as an OPML document that [`parse`] reads back identically.
///
/// A feed's first tag becomes its folder, which is the most an OPML folder can
/// express — folders nest, but a feed lives in exactly one.
pub fn write(feeds: &[FeedSource]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <opml version=\"2.0\">\n\
         \x20 <head>\n\
         \x20   <title>rsst subscriptions</title>\n\
         \x20 </head>\n\
         \x20 <body>\n",
    );
    let outline = |feed: &FeedSource, indent: &str| {
        let title = feed.title.clone().unwrap_or_else(|| feed.url.clone());
        format!(
            "{indent}<outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\"/>\n",
            escape(&title),
            escape(&title),
            escape(&feed.url),
        )
    };

    for feed in feeds.iter().filter(|f| f.tags.is_empty()) {
        out.push_str(&outline(feed, "    "));
    }

    // Group order follows first appearance, so an export is stable.
    let mut seen: Vec<&str> = Vec::new();
    for feed in feeds {
        if let Some(tag) = feed.tags.first().map(String::as_str)
            && !seen.contains(&tag)
        {
            seen.push(tag);
        }
    }
    for tag in seen {
        out.push_str(&format!("    <outline text=\"{}\">\n", escape(tag)));
        for feed in feeds
            .iter()
            .filter(|f| f.tags.first().map(String::as_str) == Some(tag))
        {
            out.push_str(&outline(feed, "      "));
        }
        out.push_str("    </outline>\n");
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
            refresh_minutes: None,
            title: title.map(Into::into),
            tags: Vec::new(),
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
    fn folders_become_tags_not_feeds() {
        let feeds = parse(REAL_WORLD).expect("should parse");
        assert!(
            !feeds.iter().any(|f| f.title.as_deref() == Some("Rust")),
            "the folder outline became a feed"
        );
        assert_eq!(feeds.len(), 3);
        assert_eq!(feeds[0].tags, ["Rust"]);
        assert_eq!(feeds[1].tags, ["Rust"]);
        assert!(feeds[2].tags.is_empty(), "a top-level feed has no folder");
    }

    #[test]
    fn nested_folders_all_become_tags() {
        let xml = r#"<opml><body>
            <outline text="Outer">
              <outline text="Inner">
                <outline type="rss" xmlUrl="https://deep.example/feed"/>
              </outline>
              <outline type="rss" xmlUrl="https://shallow.example/feed"/>
            </outline>
            <outline type="rss" xmlUrl="https://top.example/feed"/>
        </body></opml>"#;
        let feeds = parse(xml).expect("should parse");
        assert_eq!(feeds[0].tags, ["Outer", "Inner"]);
        assert_eq!(feeds[1].tags, ["Outer"], "stack unwound after Inner closed");
        assert!(feeds[2].tags.is_empty(), "stack unwound after Outer closed");
    }

    #[test]
    fn a_folder_round_trips_through_export_and_import() {
        let original = vec![
            FeedSource {
                url: "https://a.example/feed".into(),
                refresh_minutes: None,
                title: Some("A".into()),
                tags: vec!["News".into()],
            },
            FeedSource {
                url: "https://b.example/feed".into(),
                refresh_minutes: None,
                title: Some("B".into()),
                tags: Vec::new(),
            },
        ];
        let reparsed = parse(&write(&original)).expect("should parse");

        let a = reparsed
            .iter()
            .find(|f| f.url == "https://a.example/feed")
            .expect("a");
        let b = reparsed
            .iter()
            .find(|f| f.url == "https://b.example/feed")
            .expect("b");
        assert_eq!(a.tags, ["News"]);
        assert!(b.tags.is_empty());
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
