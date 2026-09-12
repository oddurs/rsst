//! Fills a throwaway rsst home with known content.
//!
//! Points `RSST_HOME` at a directory, writes a config listing the fixture
//! feeds, fetches them through the real code — the same client, the same
//! parser, the same store — and then marks a fixed pattern of entries read and
//! starred so the interface has something to look at on the first frame.
//!
//! Fetching rather than inserting rows on purpose: a seeded database that
//! never went through `fetch` would not prove the fetch path still works, and
//! that is most of what there is to get wrong.
//!
//!   cargo run --bin rsst-seed -- --home /tmp/rsst-dev --port 8787
//!
//! Every choice it makes is a function of the content, so two runs against the
//! same fixtures produce the same database.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

use rsst::config::FeedSource;
use rsst::db::Db;
use rsst::feed;

/// Every third entry has been read, every seventh starred.
///
/// Fixed rather than random: the point of a seeded database is that it is the
/// same one tomorrow.
const READ_EVERY: usize = 3;
const STARRED_EVERY: usize = 7;

/// The fixture feeds, and where they sit in the sidebar.
fn sources(port: u16) -> Vec<FeedSource> {
    let feed = |path: &str, title: &str, tags: &[&str]| FeedSource {
        url: format!("http://127.0.0.1:{port}/{path}"),
        title: Some(title.to_string()),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        refresh_minutes: None,
    };
    vec![
        feed("handbook.xml", "The Renderer Handbook", &["Reading"]),
        feed("unicode.xml", "Glyphs", &["Reading"]),
        feed("awkward.xml", "Awkward Publishing", &["Reading", "Hostile"]),
        feed("truncated.xml", "Truncated", &["Reading", "Hostile"]),
        feed("not-a-feed.xml", "Not A Feed", &["Reading", "Hostile"]),
        feed("gone.xml", "Gone (404)", &["Broken"]),
        feed("broken.xml", "Broken (500)", &["Broken"]),
        feed("empty.xml", "Nothing Yet", &[]),
        feed("medium.xml", "A Hundred Things", &["Scale"]),
        feed("huge.xml", "Ten Thousand Things", &["Scale"]),
    ]
}

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(found) = args.next() {
        if found == name {
            return args.next();
        }
        if let Some(rest) = found.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    let home = arg("--home")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(rsst::home::HOME).map(PathBuf::from))
        .context("pass --home <DIR>, or set RSST_HOME")?;
    let port: u16 = arg("--port").unwrap_or_else(|| "8787".into()).parse()?;

    rsst::home::ensure(&home)?;
    let sources = sources(port);
    write_config(&home, &sources)?;

    // A fresh database every time, so seeding is not additive and the result
    // does not depend on what was there before.
    let path = home.join("rsst.sqlite3");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    let mut db = Db::open(&path)?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("rsst-seed/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?;

    let mut read: HashSet<String> = HashSet::new();
    let mut starred: HashSet<String> = HashSet::new();
    let mut entries = 0usize;
    let mut failures = 0usize;

    for source in &sources {
        match feed::fetch(&client, source, None, None).await {
            Ok(feed::Outcome::Updated {
                feed,
                etag,
                last_modified,
            }) => {
                db.put_feed(&feed)?;
                db.set_validators(&source.url, etag, last_modified)?;
                for (index, entry) in feed.entries.iter().enumerate() {
                    entries += 1;
                    let Some(key) = entry.keys.first() else {
                        continue;
                    };
                    if index % READ_EVERY == 0 {
                        read.insert(key.clone());
                    }
                    if index % STARRED_EVERY == 1 {
                        starred.insert(key.clone());
                    }
                }
                println!(
                    "  {:<26} {} entries",
                    short(&source.url),
                    feed.entries.len()
                );
            }
            Ok(other) => println!("  {:<26} {other:?}", short(&source.url)),
            // A feed that cannot be fetched is part of the fixture set, not a
            // reason to stop: the reader has to survive one, so must seeding.
            Err(err) => {
                failures += 1;
                println!(
                    "  {:<26} unavailable ({})",
                    short(&source.url),
                    first_line(&err)
                );
            }
        }
    }

    db.save_state(&read, &HashSet::new(), &starred, &HashSet::new())?;

    println!();
    println!("seeded {}", home.display());
    println!(
        "  {entries} entries, {} read, {} starred, {failures} feed(s) unavailable",
        read.len(),
        starred.len()
    );
    Ok(())
}

/// The last part of a URL, which is the part that differs.
fn short(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

fn first_line(err: &anyhow::Error) -> String {
    err.to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Writes the config the reader will be started with.
fn write_config(home: &std::path::Path, sources: &[FeedSource]) -> Result<()> {
    let mut out = String::from(
        "# Written by rsst-seed. Edit freely — `scripts/dev seed` overwrites it.\n\
         measure = 72\n\
         # No timer: a fixture feed never changes, and a refresh mid-look is a\n\
         # frame you did not ask for.\n\
         refresh_minutes = 0\n\n",
    );
    for source in sources {
        out.push_str("[[feeds]]\n");
        out.push_str(&format!("url = \"{}\"\n", source.url));
        if let Some(title) = &source.title {
            out.push_str(&format!("title = \"{title}\"\n"));
        }
        if !source.tags.is_empty() {
            let tags: Vec<String> = source.tags.iter().map(|t| format!("\"{t}\"")).collect();
            out.push_str(&format!("tags = [{}]\n", tags.join(", ")));
        }
        out.push('\n');
    }
    let path = home.join("config.toml");
    std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))
}
