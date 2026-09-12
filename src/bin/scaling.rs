//! Does refresh cost follow the feed, or the whole backlog?
//!
//! The question this milestone exists to answer. Refreshes one feed with a
//! small corpus stored, then with a large one, and compares.

use std::time::Instant;

use rsst::config::FeedSource;

fn feed_xml(entries: usize, salt: usize) -> String {
    let items: String = (0..entries)
        .map(|i| {
            format!(
                "<item><title>Entry {salt}-{i}</title><guid>urn:{salt}-{i}</guid>\
                 <link>https://example.com/{salt}/{i}</link>\
                 <description>Body of entry {i} with enough prose to be realistic.</description></item>"
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Bench</title>{items}</channel></rss>"
    )
}

fn measure(feeds: usize) -> (f64, usize) {
    let path = std::env::temp_dir().join(format!("rsst-scaling-{feeds}.sqlite3"));
    let _ = std::fs::remove_file(&path);
    let mut db = rsst::db::Db::open(&path).expect("opens");

    let mut first = None;
    for i in 0..feeds {
        let source = FeedSource {
            url: format!("https://bench.example/{i}"),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        };
        let feed = rsst::feed::parse(feed_xml(100, i).as_bytes(), &source).expect("parses");
        db.put_feed(&feed).expect("put");
        if i == 0 {
            first = Some(feed);
        }
    }

    let one = first.expect("at least one feed");
    let runs = 20;
    let start = Instant::now();
    for _ in 0..runs {
        db.put_feed(&one).expect("put");
    }
    let per = start.elapsed().as_secs_f64() * 1e3 / f64::from(runs);
    let _ = std::fs::remove_file(&path);
    (per, feeds * 100)
}

fn main() {
    println!(
        "{:<10} {:>10} {:>18}",
        "feeds", "entries", "refresh one feed"
    );
    let mut baseline = None;
    for feeds in [10usize, 50, 200] {
        let (ms, entries) = measure(feeds);
        let ratio = baseline.map(|b: f64| ms / b).unwrap_or(1.0);
        baseline.get_or_insert(ms);
        println!("{feeds:<10} {entries:>10} {ms:>15.2} ms   {ratio:.2}x");
    }
}
