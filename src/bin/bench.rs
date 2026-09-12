//! Timings for the operations that get slower as a backlog grows.
//!
//! Run with `cargo run --release --bin rsst-bench`. Compares against the
//! baselines in `benches/baseline.toml` and exits non-zero if anything has
//! regressed by more than the tolerance there.
//!
//! The point is not nanosecond precision. It is noticing that something has
//! stopped being linear — a scroll that is fine at ten feeds and quadratic at
//! five hundred. The tolerance is deliberately generous so the check is about
//! order of magnitude rather than machine speed.

use std::time::{Duration, Instant};

use rsst::config::FeedSource;

/// One measured operation.
struct Measurement {
    name: &'static str,
    per_op: Duration,
}

fn measure(name: &'static str, iterations: u32, mut work: impl FnMut()) -> Measurement {
    // One untimed pass, so a cold allocator or cache is not counted.
    work();
    let start = Instant::now();
    for _ in 0..iterations {
        work();
    }
    Measurement {
        name,
        per_op: start.elapsed() / iterations,
    }
}

/// A feed document with `entries` items, as a server would send it.
fn feed_xml(entries: usize) -> String {
    let mut out =
        String::from("<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Bench</title>");
    for i in 0..entries {
        out.push_str(&format!(
            "<item><title>Entry number {i}</title><guid>urn:{i}</guid>\
             <link>https://example.com/{i}</link>\
             <pubDate>Wed, 01 Jan 2020 00:00:00 GMT</pubDate>\
             <description>&lt;p&gt;Body of entry {i}, with enough words in it to be \
             worth wrapping and stripping and generally handling.&lt;/p&gt;</description></item>"
        ));
    }
    out.push_str("</channel></rss>");
    out
}

fn main() {
    let baseline: toml::Table =
        toml::from_str(include_str!("../../benches/baseline.toml")).expect("baseline.toml parses");
    let tolerance = baseline
        .get("tolerance")
        .and_then(|v| v.as_float())
        .unwrap_or(3.0);

    let xml = feed_xml(500);
    let long_text = "lorem ipsum dolor sit amet ".repeat(400);

    let source = FeedSource {
        url: "https://bench.example/feed".into(),
        title: None,
        tags: Vec::new(),
    };

    // Fifty feeds of a hundred entries: a large but not absurd subscription
    // list, and the size at which anything quadratic becomes obvious.
    let hundred = feed_xml(100);
    let mut feeds = Vec::new();
    let bench_db = std::env::temp_dir().join("rsst-bench.sqlite3");
    let _ = std::fs::remove_file(&bench_db);
    let mut db = rsst::db::Db::open(&bench_db).expect("opens");
    for i in 0..50 {
        let source = FeedSource {
            url: format!("https://bench.example/{i}"),
            title: Some(format!("Feed {i}")),
            tags: Vec::new(),
        };
        let feed = rsst::feed::parse(hundred.as_bytes(), &source).expect("parses");
        db.put_feed(&feed).expect("put");
        feeds.push(feed);
    }
    let app = rsst::app::App::new(feeds.clone(), rsst::state::ReadState::default());

    let measurements = vec![
        // Parsing dominates startup once the cache is warm.
        measure("parse_500_entries", 20, || {
            let _ = rsst::feed::parse(xml.as_bytes(), &source).expect("parses");
        }),
        // Wrapping runs on every detail-pane frame.
        measure("wrap_10k_chars", 200, || {
            let _ = rsst::text::wrap(&long_text, 78);
        }),
        // Sorting runs whenever the all-feeds view is open.
        measure("sort_5000_entries", 20, || {
            let _ = app.all_entries();
        }),
        // The number this milestone is about: refreshing ONE feed when fifty
        // are stored. With the old TOML cache this rewrote all fifty.
        measure("refresh_one_of_50_feeds", 20, || {
            db.put_feed(&feeds[0]).expect("put");
        }),
        // Search across the whole corpus, through the full-text index rather
        // than a scan over everything in memory.
        measure("search_5000_entries", 50, || {
            let _ = db.search("entry", 500).expect("search");
        }),
    ];

    let mut failed = Vec::new();
    println!(
        "{:<24} {:>12} {:>12} {:>8}",
        "operation", "measured", "baseline", "ratio"
    );
    for m in &measurements {
        let micros = m.per_op.as_secs_f64() * 1e6;
        let expected = baseline
            .get(m.name)
            .and_then(|v| v.as_float())
            .unwrap_or(f64::INFINITY);
        let ratio = micros / expected;
        println!(
            "{:<24} {micros:>11.1}µ {expected:>11.1}µ {ratio:>7.2}x",
            m.name
        );
        if ratio > tolerance {
            failed.push(format!(
                "{} is {ratio:.2}x its baseline (tolerance {tolerance:.1}x)",
                m.name
            ));
        }
    }

    if !failed.is_empty() {
        eprintln!("\nregression:");
        for line in &failed {
            eprintln!("  {line}");
        }
        std::process::exit(1);
    }
    println!("\nwithin {tolerance:.1}x of baseline");
}
