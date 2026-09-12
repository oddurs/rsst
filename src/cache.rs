//! Last-known feed contents, so a launch does not have to wait for the network.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::FeedSource;
use crate::feed::Feed;

/// How many entries are kept per feed.
///
/// A feed that publishes a rolling window never grows past this anyway; one that
/// keeps its whole archive in a single document otherwise would. The cache is a
/// convenience, not an archive — entries beyond this are re-fetchable.
const MAX_ENTRIES_PER_FEED: usize = 500;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    /// Keyed by feed URL, which is what identifies a feed in the config.
    #[serde(default)]
    feeds: HashMap<String, Feed>,
}

impl Cache {
    /// Reads the cache, treating anything unreadable as simply absent.
    ///
    /// A cache that cannot be parsed is worth nothing and costs a refetch, so
    /// there is no case where failing to start is the better outcome.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&raw).unwrap_or_default()
    }

    /// Writes the cache atomically, so a crash mid-write cannot corrupt it.
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("cache path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("creating cache directory {}", parent.display()))?;

        let body = toml::to_string(self).context("serializing cache")?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
    }

    /// Records a feed's contents, trimming it to the per-feed cap.
    pub fn put(&mut self, feed: &Feed) {
        let mut feed = feed.clone();
        feed.loading = false;
        feed.entries.truncate(MAX_ENTRIES_PER_FEED);
        self.feeds.insert(feed.url.clone(), feed);
    }

    /// The cached contents of `source`, if any, ready to display.
    pub fn get(&self, source: &FeedSource) -> Option<Feed> {
        let mut feed = self.feeds.get(&source.url).cloned()?;
        // The config is the authority on what a feed is called.
        if let Some(title) = &source.title {
            feed.title = title.clone();
        }
        // Still in flight — the cached copy is what is shown until it lands.
        feed.loading = true;
        Some(feed)
    }

    /// Drops feeds that are no longer configured.
    ///
    /// Without this, removing a feed from the config leaves its entries on disk
    /// for good, and the cache only ever grows.
    pub fn retain_configured(&mut self, sources: &[FeedSource]) {
        self.feeds
            .retain(|url, _| sources.iter().any(|source| &source.url == url));
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.feeds.len()
    }
}

/// `$XDG_CACHE_HOME/rsst/feeds.toml`, or the platform equivalent.
pub fn cache_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "rsst")
        .context("could not determine a cache directory for this platform")?;
    Ok(dirs.cache_dir().join("feeds.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::Entry;

    fn entry(title: &str) -> Entry {
        Entry {
            title: title.into(),
            link: None,
            published: None,
            summary: String::new(),
            keys: vec![format!("id:{title}")],
        }
    }

    fn feed(url: &str, entries: usize) -> Feed {
        Feed {
            title: "Title".into(),
            url: url.into(),
            loading: false,
            entries: (0..entries).map(|i| entry(&format!("e{i}"))).collect(),
        }
    }

    fn source(url: &str) -> FeedSource {
        FeedSource {
            url: url.into(),
            title: None,
        }
    }

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rsst-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_stored_feed_comes_back() {
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", 3));
        let restored = cache
            .get(&source("https://a.example/feed"))
            .expect("cached");
        assert_eq!(restored.entries.len(), 3);
    }

    #[test]
    fn an_unknown_feed_is_absent() {
        assert!(
            Cache::default()
                .get(&source("https://nope.example"))
                .is_none()
        );
    }

    #[test]
    fn a_restored_feed_is_marked_loading_because_a_refresh_is_coming() {
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", 1));
        assert!(
            cache
                .get(&source("https://a.example/feed"))
                .expect("cached")
                .loading
        );
    }

    #[test]
    fn the_configured_title_overrides_the_cached_one() {
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", 1));
        let mut source = source("https://a.example/feed");
        source.title = Some("My Name".into());
        assert_eq!(cache.get(&source).expect("cached").title, "My Name");
    }

    #[test]
    fn entries_are_capped_so_the_cache_cannot_grow_without_bound() {
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", MAX_ENTRIES_PER_FEED + 250));
        assert_eq!(
            cache
                .get(&source("https://a.example/feed"))
                .expect("cached")
                .entries
                .len(),
            MAX_ENTRIES_PER_FEED
        );
    }

    #[test]
    fn feeds_removed_from_the_config_are_dropped() {
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", 1));
        cache.put(&feed("https://b.example/feed", 1));
        assert_eq!(cache.len(), 2);

        cache.retain_configured(&[source("https://a.example/feed")]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&source("https://b.example/feed")).is_none());
    }

    #[test]
    fn the_cache_round_trips_through_a_file() {
        let path = tmpdir().join("round-trip.toml");
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", 2));
        cache.save(&path).expect("save");

        let loaded = Cache::load(&path);
        let restored = loaded
            .get(&source("https://a.example/feed"))
            .expect("cached");
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].title, "e0");
    }

    #[test]
    fn a_corrupt_cache_is_discarded_rather_than_fatal() {
        let path = tmpdir().join("corrupt.toml");
        fs::write(&path, "{{{ not toml at all").expect("write");
        assert_eq!(Cache::load(&path).len(), 0);
    }

    #[test]
    fn a_truncated_cache_is_discarded_rather_than_fatal() {
        let path = tmpdir().join("truncated.toml");
        let mut cache = Cache::default();
        cache.put(&feed("https://a.example/feed", 5));
        cache.save(&path).expect("save");

        let full = fs::read_to_string(&path).expect("read");
        fs::write(&path, &full[..full.len() / 2]).expect("truncate");

        assert_eq!(Cache::load(&path).len(), 0);
    }

    #[test]
    fn a_missing_cache_is_empty() {
        assert_eq!(
            Cache::load(Path::new("/nonexistent/rsst/feeds.toml")).len(),
            0
        );
    }

    #[test]
    fn saving_leaves_no_temporary_file_behind() {
        let dir = tmpdir();
        let path = dir.join("clean.toml");
        Cache::default().save(&path).expect("save");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
