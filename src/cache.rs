//! Last-known feed contents, so a launch does not have to wait for the network.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::FeedSource;
use crate::feed::Feed;
use crate::state::ReadState;

/// How many entries are kept per feed.
///
/// A feed that publishes a rolling window never grows past this anyway; one that
/// keeps its whole archive in a single document otherwise would. The cache is a
/// convenience, not an archive — entries beyond this are re-fetchable.
const MAX_ENTRIES_PER_FEED: usize = 500;

/// The format version written to the cache file.
///
/// A cache from a newer rsst is discarded, which costs one refetch and nothing
/// else — the cache is a convenience, never the only copy of anything.
pub const FORMAT: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    /// Absent in files written before versioning; treated as version 1.
    #[serde(default = "one")]
    version: u32,
    /// Keyed by feed URL, which is what identifies a feed in the config.
    #[serde(default)]
    feeds: HashMap<String, Feed>,
    /// HTTP conditional-request state, kept apart from the display model.
    #[serde(default)]
    meta: HashMap<String, FeedMeta>,
}

/// What we remember about a feed's HTTP behaviour between fetches.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FeedMeta {
    /// `ETag`, replayed as `If-None-Match`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// `Last-Modified`, replayed as `If-Modified-Since`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// Earliest time we may ask again, set by `Retry-After`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<DateTime<Utc>>,
}

fn one() -> u32 {
    1
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
        let cache: Self = toml::from_str(&raw).unwrap_or_default();
        if cache.version > FORMAT {
            return Self::default();
        }
        cache
    }

    /// Writes the cache atomically, so a crash mid-write cannot corrupt it.
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("cache path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("creating cache directory {}", parent.display()))?;

        let mut current = Self {
            version: FORMAT,
            feeds: self.feeds.clone(),
            meta: self.meta.clone(),
        };
        current.version = FORMAT;
        let body = toml::to_string(&current).context("serializing cache")?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
    }

    /// Records a feed's contents, keeping starred entries the feed has dropped.
    ///
    /// A publisher's feed is a rolling window; starring is the reader saying
    /// they want something after it rolls out. Those entries are carried
    /// forward and are exempt from the per-feed cap, since capping them away
    /// would defeat the point of starring.
    pub fn put(&mut self, feed: &Feed, state: &ReadState) {
        let mut next = feed.clone();
        next.status = crate::feed::Status::Idle;
        next.entries.truncate(MAX_ENTRIES_PER_FEED);

        if let Some(previous) = self.feeds.get(&next.url) {
            let kept: Vec<_> = previous
                .entries
                .iter()
                .filter(|entry| {
                    state.any_starred(&entry.keys)
                        && !next
                            .entries
                            .iter()
                            .any(|e| e.keys.iter().any(|key| entry.keys.contains(key)))
                })
                .cloned()
                .collect();
            next.entries.extend(kept);
        }

        self.feeds.insert(next.url.clone(), next);
    }

    #[cfg(test)]
    fn put_test(&mut self, feed: &Feed) {
        self.put(feed, &ReadState::default());
    }

    /// The cached contents of `source`, if any, ready to display.
    pub fn get(&self, source: &FeedSource) -> Option<Feed> {
        let mut feed = self.feeds.get(&source.url).cloned()?;
        // The config is the authority on what a feed is called.
        if let Some(title) = &source.title {
            feed.title = title.clone();
        }
        // Still in flight — the cached copy is what is shown until it lands.
        feed.status = crate::feed::Status::Fetching;
        Some(feed)
    }

    /// Drops feeds that are no longer configured.
    ///
    /// Without this, removing a feed from the config leaves its entries on disk
    /// for good, and the cache only ever grows.
    pub fn retain_configured(&mut self, sources: &[FeedSource]) {
        self.feeds
            .retain(|url, _| sources.iter().any(|source| &source.url == url));
        self.meta
            .retain(|url, _| sources.iter().any(|source| &source.url == url));
    }

    /// Every cached feed, for migrating the old file into the database.
    pub fn feeds(self) -> Vec<Feed> {
        self.feeds.into_values().collect()
    }

    /// What we know about this feed's HTTP behaviour.
    pub fn meta(&self, url: &str) -> FeedMeta {
        self.meta.get(url).cloned().unwrap_or_default()
    }

    /// Records the validators a response carried, clearing any deferral.
    pub fn set_validators(
        &mut self,
        url: &str,
        etag: Option<String>,
        last_modified: Option<String>,
    ) {
        let entry = self.meta.entry(url.to_string()).or_default();
        // Only overwrite with something: a response that omits a validator it
        // sent last time should not cost us the one we have.
        if etag.is_some() {
            entry.etag = etag;
        }
        if last_modified.is_some() {
            entry.last_modified = last_modified;
        }
        entry.retry_after = None;
    }

    /// Records that the server asked us to wait.
    pub fn defer_until(&mut self, url: &str, until: DateTime<Utc>) {
        self.meta.entry(url.to_string()).or_default().retry_after = Some(until);
    }

    /// Whether we are allowed to fetch this feed yet.
    pub fn may_fetch(&self, url: &str, now: DateTime<Utc>) -> bool {
        match self.meta.get(url).and_then(|m| m.retry_after) {
            Some(until) => now >= until,
            None => true,
        }
    }

    /// How many feeds are held. A test helper, deliberately not named `len`:
    /// this is not a collection, and clippy is right that a `len` without
    /// an `is_empty` reads as one.
    #[cfg(test)]
    pub fn count(&self) -> usize {
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
            status: crate::feed::Status::Idle,
            entries: (0..entries).map(|i| entry(&format!("e{i}"))).collect(),
        }
    }

    fn source(url: &str) -> FeedSource {
        FeedSource {
            url: url.into(),
            title: None,
            tags: Vec::new(),
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
        cache.put_test(&feed("https://a.example/feed", 3));
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
    fn a_restored_feed_is_marked_fetching_because_a_refresh_is_coming() {
        let mut cache = Cache::default();
        cache.put_test(&feed("https://a.example/feed", 1));
        assert_eq!(
            cache
                .get(&source("https://a.example/feed"))
                .expect("cached")
                .status,
            crate::feed::Status::Fetching
        );
    }

    #[test]
    fn the_configured_title_overrides_the_cached_one() {
        let mut cache = Cache::default();
        cache.put_test(&feed("https://a.example/feed", 1));
        let mut source = source("https://a.example/feed");
        source.title = Some("My Name".into());
        assert_eq!(cache.get(&source).expect("cached").title, "My Name");
    }

    #[test]
    fn entries_are_capped_so_the_cache_cannot_grow_without_bound() {
        let mut cache = Cache::default();
        cache.put_test(&feed("https://a.example/feed", MAX_ENTRIES_PER_FEED + 250));
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
        cache.put_test(&feed("https://a.example/feed", 1));
        cache.put_test(&feed("https://b.example/feed", 1));
        assert_eq!(cache.count(), 2);

        cache.retain_configured(&[source("https://a.example/feed")]);
        assert_eq!(cache.count(), 1);
        assert!(cache.get(&source("https://b.example/feed")).is_none());
    }

    #[test]
    fn validators_are_remembered_and_survive_a_restart() {
        let path = tmpdir().join("validators.toml");
        let mut cache = Cache::default();
        cache.set_validators("https://a.example/feed", Some("\"abc\"".into()), None);
        cache.save(&path).expect("save");

        let meta = Cache::load(&path).meta("https://a.example/feed");
        assert_eq!(meta.etag.as_deref(), Some("\"abc\""));
    }

    #[test]
    fn a_response_missing_a_validator_does_not_erase_the_stored_one() {
        let mut cache = Cache::default();
        cache.set_validators("u", Some("\"abc\"".into()), Some("Mon".into()));
        cache.set_validators("u", None, None);
        assert_eq!(cache.meta("u").etag.as_deref(), Some("\"abc\""));
        assert_eq!(cache.meta("u").last_modified.as_deref(), Some("Mon"));
    }

    #[test]
    fn a_deferred_feed_may_not_be_fetched_until_its_time() {
        let now = Utc::now();
        let mut cache = Cache::default();
        cache.defer_until("u", now + chrono::Duration::seconds(60));

        assert!(!cache.may_fetch("u", now));
        assert!(cache.may_fetch("u", now + chrono::Duration::seconds(61)));
    }

    #[test]
    fn a_feed_we_know_nothing_about_may_always_be_fetched() {
        assert!(Cache::default().may_fetch("u", Utc::now()));
    }

    #[test]
    fn a_successful_response_clears_a_deferral() {
        let now = Utc::now();
        let mut cache = Cache::default();
        cache.defer_until("u", now + chrono::Duration::seconds(60));
        cache.set_validators("u", Some("\"x\"".into()), None);
        assert!(cache.may_fetch("u", now));
    }

    #[test]
    fn metadata_for_removed_feeds_is_dropped_too() {
        let mut cache = Cache::default();
        cache.set_validators("https://gone.example/feed", Some("\"x\"".into()), None);
        cache.retain_configured(&[source("https://a.example/feed")]);
        assert!(cache.meta("https://gone.example/feed").etag.is_none());
    }

    #[test]
    fn the_cache_round_trips_through_a_file() {
        let path = tmpdir().join("round-trip.toml");
        let mut cache = Cache::default();
        cache.put_test(&feed("https://a.example/feed", 2));
        cache.save(&path).expect("save");

        let loaded = Cache::load(&path);
        let restored = loaded
            .get(&source("https://a.example/feed"))
            .expect("cached");
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].title, "e0");
    }

    #[test]
    fn a_starred_entry_survives_falling_out_of_the_feed() {
        let mut state = ReadState::default();
        let mut cache = Cache::default();

        let original = feed("https://a.example/feed", 3);
        state.toggle_star(&original.entries[1]);
        cache.put(&original, &state);

        // The publisher rolls the window: only a brand new entry remains.
        let mut rolled = feed("https://a.example/feed", 0);
        rolled.entries = vec![entry("brand-new")];
        cache.put(&rolled, &state);

        let restored = cache
            .get(&source("https://a.example/feed"))
            .expect("cached");
        let titles: Vec<_> = restored.entries.iter().map(|e| e.title.as_str()).collect();
        assert!(titles.contains(&"brand-new"));
        assert!(titles.contains(&"e1"), "the starred entry was kept");
        assert!(!titles.contains(&"e0"), "unstarred entries rolled away");
    }

    #[test]
    fn a_starred_entry_still_present_is_not_duplicated() {
        let mut state = ReadState::default();
        let mut cache = Cache::default();
        let original = feed("https://a.example/feed", 2);
        state.toggle_star(&original.entries[0]);

        cache.put(&original, &state);
        cache.put(&original, &state);

        let restored = cache
            .get(&source("https://a.example/feed"))
            .expect("cached");
        assert_eq!(restored.entries.len(), 2);
    }

    #[test]
    fn the_cache_records_its_format_version() {
        let path = tmpdir().join("versioned.toml");
        Cache::default().save(&path).expect("save");
        let raw = fs::read_to_string(&path).expect("read");
        assert!(raw.contains(&format!("version = {FORMAT}")));
    }

    #[test]
    fn a_cache_from_a_newer_rsst_is_discarded() {
        let path = tmpdir().join("from-the-future.toml");
        let mut cache = Cache::default();
        cache.put_test(&feed("https://a.example/feed", 2));
        cache.save(&path).expect("save");
        let raw = fs::read_to_string(&path).expect("read");
        fs::write(
            &path,
            raw.replace(
                &format!("version = {FORMAT}"),
                &format!("version = {}", FORMAT + 1),
            ),
        )
        .expect("write");

        assert_eq!(Cache::load(&path).count(), 0);
    }

    #[test]
    fn a_corrupt_cache_is_discarded_rather_than_fatal() {
        let path = tmpdir().join("corrupt.toml");
        fs::write(&path, "{{{ not toml at all").expect("write");
        assert_eq!(Cache::load(&path).count(), 0);
    }

    #[test]
    fn a_truncated_cache_is_discarded_rather_than_fatal() {
        let path = tmpdir().join("truncated.toml");
        let mut cache = Cache::default();
        cache.put_test(&feed("https://a.example/feed", 5));
        cache.save(&path).expect("save");

        // Cut inside a quoted value, which is what a partial write looks like
        // and which no amount of TOML tolerance can recover.
        let full = fs::read_to_string(&path).expect("read");
        let cut = full.find("title = \"").expect("a quoted value") + 9;
        fs::write(&path, &full[..cut]).expect("truncate");

        assert_eq!(Cache::load(&path).count(), 0);
    }

    #[test]
    fn a_missing_cache_is_empty() {
        assert_eq!(
            Cache::load(Path::new("/nonexistent/rsst/feeds.toml")).count(),
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
