//! Everything rsst remembers, in one SQLite database.
//!
//! Replaces the two TOML files this used to keep. The reason is not that SQLite
//! is nicer but that the files were rewritten whole on every refresh: the cost
//! was linear in the *total* backlog rather than in what changed. Rows can be
//! written, deleted and searched one at a time.
//!
//! Read and starred state is also mirrored in memory. Rendering asks "is this
//! entry read?" once per visible row per frame, and a query per cell would be
//! absurd — so the set is loaded once and every change is written through.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::config::FeedSource;
use crate::feed::{Entry, Feed, Status};

/// The schema version this build writes.
///
/// Stored in SQLite's own `user_version`, so the database carries its version
/// the way `docs/stability.md` requires — and, as with the TOML before it, a
/// database from a newer rsst is refused rather than misread.
pub const SCHEMA: i64 = 6;

/// What we remember about a feed's HTTP behaviour between fetches.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FeedMeta {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub retry_after: Option<DateTime<Utc>>,
}

pub struct Db {
    connection: Connection,
    /// Mirror of `read_keys`, for rendering.
    read: HashSet<String>,
    /// Mirror of `starred_keys`.
    starred: HashSet<String>,
    /// True if this run brought the old TOML across.
    migrated_this_run: bool,
    /// Rows touched by the last `put_feed`, for the tests that check the
    /// delta is actually a delta.
    last_written: usize,
}

impl std::fmt::Debug for Db {
    /// Names the sets rather than the connection, which has nothing useful to
    /// show and cannot be formatted.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db")
            .field("read", &self.read.len())
            .field("starred", &self.starred.len())
            .finish_non_exhaustive()
    }
}

impl Db {
    /// Opens the database, creating and migrating it as needed.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        // WAL so a refresh writing rows does not block the read that draws the
        // next frame. Foreign keys so deleting a feed takes its entries.
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .context("enabling WAL")?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .context("enabling foreign keys")?;

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .context("reading the schema version")?;
        if version > SCHEMA {
            anyhow::bail!("this database is version {version}, and this rsst understands {SCHEMA}");
        }
        if version < SCHEMA {
            migrate(&connection, version)?;
        }

        let mut db = Self {
            connection,
            read: HashSet::new(),
            starred: HashSet::new(),
            migrated_this_run: false,
            last_written: 0,
        };
        db.read = db.keys_in("read_keys")?;
        db.starred = db.keys_in("starred_keys")?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn keys_in(&self, table: &str) -> Result<HashSet<String>> {
        let mut statement = self
            .connection
            .prepare(&format!("SELECT key FROM {table}"))?;
        let keys = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<String>>>()?;
        Ok(keys)
    }

    // ─── Feeds and entries ───────────────────────────────────────────────

    /// Replaces a feed's entries, keeping starred ones that have rolled out.
    ///
    /// One transaction, and only this feed's rows — which is the whole point:
    /// refreshing one feed no longer rewrites the other forty-nine.
    /// Stores a feed, writing only what actually changed.
    ///
    /// It used to delete every row for the feed and insert them all again,
    /// which cost 654 ms for a five-thousand-entry feed and ran between two
    /// frames — the interface froze for two-thirds of a second whenever a
    /// large feed returned anything a conditional request had not ruled out.
    ///
    /// `CLAUDE.md` already said so about read state: written back as deltas,
    /// never as a whole-table rewrite, because the rewrite is what the TOML
    /// files did and why they did not scale. Entries were still doing it.
    pub fn put_feed(&mut self, feed: &Feed) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        transaction.execute(
            "INSERT INTO feeds (url, title) VALUES (?1, ?2)
             ON CONFLICT(url) DO UPDATE SET title = excluded.title",
            params![feed.url, feed.title],
        )?;

        // What is already stored, by every key it can be recognised by. Only
        // the fingerprint comes back, not the bodies: the point is to decide
        // what to write without reading thirty megabytes to find out.
        let mut stored: HashMap<String, (i64, i64, Option<i64>)> = HashMap::new();
        let mut order: Vec<(i64, i64)> = Vec::new();
        {
            let mut statement = transaction.prepare(
                "SELECT e.id, e.position, e.digest, k.key FROM entries e
                 LEFT JOIN entry_keys k ON k.entry_id = e.id
                 WHERE e.feed_url = ?1",
            )?;
            let rows = statement.query_map(params![feed.url], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?;
            let mut seen_ids: HashSet<i64> = HashSet::new();
            for row in rows {
                let (id, position, digest, key) = row?;
                if seen_ids.insert(id) {
                    order.push((position, id));
                }
                if let Some(key) = key {
                    stored.insert(key, (id, position, digest));
                }
            }
        }

        let mut kept: HashSet<i64> = HashSet::new();
        let mut written = 0usize;

        for (position, entry) in feed.entries.iter().enumerate() {
            let position = position as i64;
            let found = entry.keys.iter().find_map(|key| stored.get(key)).copied();

            match found {
                // Already stored, unchanged, and in the right place: nothing
                // to do, which is what almost every refresh should cost.
                Some((id, at, Some(digest))) if digest == digest_of(entry) => {
                    // Unchanged. If it has only moved, write the position and
                    // nothing else: rewriting the body would re-index it for
                    // a change the index cannot see.
                    if at != position {
                        transaction.execute(
                            "UPDATE entries SET position = ?2 WHERE id = ?1",
                            params![id, position],
                        )?;
                        written += 1;
                    }
                    kept.insert(id);
                }
                Some((id, ..)) => {
                    update_entry(&transaction, id, position, entry)?;
                    kept.insert(id);
                    written += 1;
                }
                None => {
                    insert_entry(&transaction, &feed.url, position, entry)?;
                    written += 1;
                }
            }
        }

        // Entries the publisher has dropped. A starred one is kept — starring
        // is the reader saying they want it after it rolls out of the feed —
        // and moves below everything still published.
        let starred: HashSet<&String> = self.starred.iter().collect();
        let mut below = feed.entries.len() as i64;
        for (at, id) in order {
            if kept.contains(&id) {
                continue;
            }
            let is_starred = transaction
                .prepare("SELECT key FROM entry_keys WHERE entry_id = ?1")?
                .query_map(params![id], |row| row.get::<_, String>(0))?
                .filter_map(Result::ok)
                .any(|key| starred.contains(&key));

            if is_starred {
                if at != below {
                    transaction.execute(
                        "UPDATE entries SET position = ?2 WHERE id = ?1",
                        params![id, below],
                    )?;
                    written += 1;
                }
                below += 1;
            } else {
                transaction.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
                written += 1;
            }
        }

        transaction.commit()?;
        self.last_written = written;
        Ok(())
    }

    /// How many rows the last [`Self::put_feed`] actually touched.
    ///
    /// Not for the interface: for the test that says an unchanged refresh
    /// writes nothing, which is the whole point of the delta.
    pub fn last_written(&self) -> usize {
        self.last_written
    }

    /// A feed's cached contents, ready to display.
    pub fn feed(&self, source: &FeedSource) -> Result<Option<Feed>> {
        let title: Option<String> = self
            .connection
            .query_row(
                "SELECT title FROM feeds WHERE url = ?1",
                params![source.url],
                |row| row.get(0),
            )
            .optional()?;
        let Some(title) = title else {
            return Ok(None);
        };

        let mut statement = self.connection.prepare(
            "SELECT id, title, link, published, summary, content FROM entries
             WHERE feed_url = ?1 ORDER BY position",
        )?;
        let rows = statement.query_map(params![source.url], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Entry {
                    title: row.get(1)?,
                    link: row.get(2)?,
                    published: row
                        .get::<_, Option<String>>(3)?
                        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                        .map(|t| t.with_timezone(&Utc)),
                    summary: row.get(4)?,
                    content: row.get(5)?,
                    keys: Vec::new(),
                },
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, mut entry) = row?;
            entry.keys = self.keys_of(id)?;
            entries.push(entry);
        }

        Ok(Some(Feed {
            // The config is the authority on what a feed is called.
            title: source.title.clone().unwrap_or(title),
            url: source.url.clone(),
            entries,
            // Still in flight: the cached copy stands until the fetch lands.
            status: Status::Fetching,
        }))
    }

    fn keys_of(&self, entry: i64) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT key FROM entry_keys WHERE entry_id = ?1")?;
        let keys = statement
            .query_map(params![entry], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(keys)
    }

    /// Drops feeds that are no longer configured, and prunes what they held.
    pub fn retain_configured(&mut self, sources: &[FeedSource]) -> Result<()> {
        let urls: Vec<&str> = sources.iter().map(|s| s.url.as_str()).collect();
        let transaction = self.connection.unchecked_transaction()?;
        if urls.is_empty() {
            transaction.execute("DELETE FROM feeds", [])?;
        } else {
            let holes = vec!["?"; urls.len()].join(",");
            let mut arguments: Vec<&dyn rusqlite::ToSql> = Vec::new();
            for url in &urls {
                arguments.push(url);
            }
            transaction.execute(
                &format!("DELETE FROM feeds WHERE url NOT IN ({holes})"),
                arguments.as_slice(),
            )?;
        }
        transaction.commit()?;
        self.prune()?;
        Ok(())
    }

    /// Forgets read and starred keys for entries nothing holds any more.
    ///
    /// The TOML version could not do this at all: it was a list that only grew.
    ///
    /// Skipped on the run that migrated from TOML. The old file holds read keys
    /// for entries that rolled out of their feeds long ago; those entries are
    /// not in the database yet and may never be, so pruning immediately would
    /// throw away the history the migration just went to the trouble of saving.
    pub fn prune(&mut self) -> Result<usize> {
        if self.migrated_this_run {
            return Ok(0);
        }
        let removed = self.connection.execute(
            "DELETE FROM read_keys WHERE key NOT IN (SELECT key FROM entry_keys)
             AND key NOT IN (SELECT key FROM starred_keys)",
            [],
        )?;
        self.read = self.keys_in("read_keys")?;
        Ok(removed)
    }

    // ─── Conditional requests ────────────────────────────────────────────

    pub fn meta(&self, url: &str) -> FeedMeta {
        self.connection
            .query_row(
                "SELECT etag, last_modified, retry_after FROM feeds WHERE url = ?1",
                params![url],
                |row| {
                    Ok(FeedMeta {
                        etag: row.get(0)?,
                        last_modified: row.get(1)?,
                        retry_after: row
                            .get::<_, Option<String>>(2)?
                            .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                            .map(|t| t.with_timezone(&Utc)),
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Records the validators a response carried, clearing any deferral.
    pub fn set_validators(
        &self,
        url: &str,
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<()> {
        // COALESCE so a response that omits a validator it sent last time does
        // not cost us the one we have.
        self.connection.execute(
            "INSERT INTO feeds (url, title, etag, last_modified, retry_after)
             VALUES (?1, ?1, ?2, ?3, NULL)
             ON CONFLICT(url) DO UPDATE SET
               etag = COALESCE(excluded.etag, feeds.etag),
               last_modified = COALESCE(excluded.last_modified, feeds.last_modified),
               retry_after = NULL",
            params![url, etag, last_modified],
        )?;
        Ok(())
    }

    pub fn defer_until(&self, url: &str, until: DateTime<Utc>) -> Result<()> {
        self.connection.execute(
            "INSERT INTO feeds (url, title, retry_after) VALUES (?1, ?1, ?2)
             ON CONFLICT(url) DO UPDATE SET retry_after = excluded.retry_after",
            params![url, until.to_rfc3339()],
        )?;
        Ok(())
    }

    /// Records that this feed was reached, whatever the answer was.
    ///
    /// A `304` counts: the server was asked and replied, which is exactly what
    /// the timer wants to know.
    /// Records where a feed was actually found, so the next fetch skips the
    /// page that named it.
    pub fn set_resolved(&self, url: &str, resolved: Option<&str>) -> Result<()> {
        self.connection
            .execute(
                "UPDATE feeds SET resolved_url = ?2 WHERE url = ?1",
                params![url, resolved],
            )
            .context("recording where the feed was found")?;
        Ok(())
    }

    /// Where to actually look for this feed: what was discovered, or the
    /// configured address when nothing was.
    pub fn resolved(&self, url: &str) -> Option<String> {
        self.connection
            .query_row(
                "SELECT resolved_url FROM feeds WHERE url = ?1",
                params![url],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .ok()
            .flatten()
            .flatten()
    }

    pub fn mark_fetched(&self, url: &str, now: DateTime<Utc>) -> Result<()> {
        self.connection.execute(
            "INSERT INTO feeds (url, title, fetched_at) VALUES (?1, ?1, ?2)
             ON CONFLICT(url) DO UPDATE SET fetched_at = excluded.fetched_at",
            params![url, now.to_rfc3339()],
        )?;
        Ok(())
    }

    /// Whether enough time has passed to fetch this feed again.
    ///
    /// A feed never fetched is due at once. A zero interval means never, which
    /// is how someone turns the timer off.
    pub fn due(&self, url: &str, interval: Duration, now: DateTime<Utc>) -> bool {
        if interval.is_zero() || !self.may_fetch(url, now) {
            return false;
        }
        let last: Option<DateTime<Utc>> = self
            .connection
            .query_row(
                "SELECT fetched_at FROM feeds WHERE url = ?1",
                params![url],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .ok()
            .flatten()
            .flatten()
            .and_then(|text| DateTime::parse_from_rfc3339(&text).ok())
            .map(|time| time.with_timezone(&Utc));

        match last {
            Some(last) => now.signed_duration_since(last).to_std().unwrap_or_default() >= interval,
            None => true,
        }
    }

    pub fn may_fetch(&self, url: &str, now: DateTime<Utc>) -> bool {
        match self.meta(url).retry_after {
            Some(until) => now >= until,
            None => true,
        }
    }

    // ─── Read and starred ────────────────────────────────────────────────

    /// Every read and starred key, for the in-memory index rendering uses.
    ///
    /// Asking the database "is this entry read?" once per visible row per frame
    /// would be absurd, so the sets are loaded once and kept in memory. Changes
    /// go back as deltas.
    pub fn load_state(&self) -> Result<(HashSet<String>, HashSet<String>)> {
        Ok((self.keys_in("read_keys")?, self.keys_in("starred_keys")?))
    }

    /// Applies what changed since the last save — not the whole set.
    pub fn save_state(
        &mut self,
        read_added: &HashSet<String>,
        read_removed: &HashSet<String>,
        starred_added: &HashSet<String>,
        starred_removed: &HashSet<String>,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for (table, added, removed) in [
            ("read_keys", read_added, read_removed),
            ("starred_keys", starred_added, starred_removed),
        ] {
            for key in added {
                transaction.execute(
                    &format!("INSERT OR IGNORE INTO {table} (key) VALUES (?1)"),
                    params![key],
                )?;
            }
            for key in removed {
                transaction
                    .execute(&format!("DELETE FROM {table} WHERE key = ?1"), params![key])?;
            }
        }
        transaction.commit()?;

        // The mirrors are what `put_feed` asks whether an entry is starred, so
        // an entry starred in this session has to reach them — or a refresh
        // that drops it from the feed would delete it despite the star.
        for key in read_added {
            self.read.insert(key.clone());
        }
        for key in read_removed {
            self.read.remove(key);
        }
        for key in starred_added {
            self.starred.insert(key.clone());
        }
        for key in starred_removed {
            self.starred.remove(key);
        }
        Ok(())
    }

    // ─── Preferences ─────────────────────────────────────────────────────

    pub fn flag(&self, name: &str) -> bool {
        self.connection
            .query_row(
                "SELECT value FROM prefs WHERE name = ?1",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
            .is_some_and(|value| value == "true")
    }

    pub fn set_flag(&self, name: &str, value: bool) {
        let _ = self.connection.execute(
            "INSERT INTO prefs (name, value) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET value = excluded.value",
            params![name, if value { "true" } else { "false" }],
        );
    }

    // ─── Fetched articles ────────────────────────────────────────────────

    /// The full article fetched for this entry, if one ever was.
    pub fn article(&self, entry: &Entry) -> Option<String> {
        entry.keys.iter().find_map(|key| {
            self.connection
                .query_row(
                    "SELECT html FROM articles WHERE key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .ok()
                .flatten()
        })
    }

    /// Stores a fetched article against every one of the entry's identifiers.
    ///
    /// All of them, for the same reason read state uses all of them: a feed
    /// that regenerates its guids would otherwise lose the article.
    pub fn put_article(&self, entry: &Entry, html: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        for key in &entry.keys {
            transaction.execute(
                "INSERT INTO articles (key, html, fetched_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET html = excluded.html,
                                                fetched_at = excluded.fetched_at",
                params![key, html, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    // ─── Search ──────────────────────────────────────────────────────────

    /// Full-text search across every cached entry.
    ///
    /// FTS5 rather than a scan over everything held in memory, so the cost
    /// follows the number of matches rather than the size of the backlog.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, i64)>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "SELECT e.feed_url, e.position FROM entries_fts f
             JOIN entries e ON e.id = f.rowid
             WHERE entries_fts MATCH ?1
             ORDER BY e.feed_url, e.position
             LIMIT ?2",
        )?;
        let hits = statement
            .query_map(params![fts_query(query), limit as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<(String, i64)>>>()?;
        Ok(hits)
    }

    #[cfg(test)]
    pub fn count(&self, table: &str) -> i64 {
        self.connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap_or(-1)
    }
}

/// Turns what someone typed into an FTS5 prefix query.
///
/// Quoted so punctuation cannot be read as FTS syntax — a search for `c++`
/// should find entries about C++, not raise a syntax error.
fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("\"{}\"*", word.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A fingerprint of everything about an entry that is stored.
///
/// FNV-1a, written out rather than taken from `DefaultHasher`, whose output is
/// explicitly not stable between Rust releases — and a fingerprint that changed
/// with the compiler would rewrite every row once for no reason.
fn digest_of(entry: &Entry) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    eat(entry.title.as_bytes());
    eat(b"\x1f");
    eat(entry.link.as_deref().unwrap_or_default().as_bytes());
    eat(b"\x1f");
    eat(entry
        .published
        .map(|when| when.to_rfc3339())
        .unwrap_or_default()
        .as_bytes());
    eat(b"\x1f");
    eat(entry.summary.as_bytes());
    eat(b"\x1f");
    eat(entry.content.as_bytes());
    hash as i64
}

/// Rewrites a stored entry in place, keeping its id.
///
/// In place rather than delete-and-insert so read state, which is matched on
/// keys, and the FTS row both follow the entry rather than being churned.
fn update_entry(connection: &Connection, id: i64, position: i64, entry: &Entry) -> Result<()> {
    connection.execute(
        "UPDATE entries SET position = ?2, title = ?3, link = ?4, published = ?5,
             summary = ?6, content = ?7, digest = ?8
         WHERE id = ?1",
        params![
            id,
            position,
            entry.title,
            entry.link,
            entry.published.map(|t| t.to_rfc3339()),
            entry.summary,
            entry.content,
            digest_of(entry)
        ],
    )?;
    // The keys can grow: a feed that starts publishing guids adds one.
    for key in &entry.keys {
        connection.execute(
            "INSERT OR IGNORE INTO entry_keys (entry_id, key) VALUES (?1, ?2)",
            params![id, key],
        )?;
    }
    Ok(())
}

fn insert_entry(
    connection: &Connection,
    feed_url: &str,
    position: i64,
    entry: &Entry,
) -> Result<()> {
    connection.execute(
        "INSERT INTO entries (feed_url, position, title, link, published, summary, content, digest)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            feed_url,
            position,
            entry.title,
            entry.link,
            entry.published.map(|t| t.to_rfc3339()),
            entry.summary,
            entry.content,
            digest_of(entry)
        ],
    )?;
    let id = connection.last_insert_rowid();
    for key in &entry.keys {
        connection.execute(
            "INSERT OR IGNORE INTO entry_keys (entry_id, key) VALUES (?1, ?2)",
            params![id, key],
        )?;
    }
    Ok(())
}

/// Brings a database up to [`SCHEMA`].
fn migrate(connection: &Connection, from: i64) -> Result<()> {
    if from < 1 {
        connection
            .execute_batch(
                "BEGIN;
                 CREATE TABLE feeds (
                   url           TEXT PRIMARY KEY,
                   title         TEXT NOT NULL,
                   etag          TEXT,
                   last_modified TEXT,
                   retry_after   TEXT
                 );
                 CREATE TABLE entries (
                   id        INTEGER PRIMARY KEY,
                   feed_url  TEXT NOT NULL REFERENCES feeds(url) ON DELETE CASCADE,
                   position  INTEGER NOT NULL,
                   title     TEXT NOT NULL,
                   link      TEXT,
                   published TEXT,
                   summary   TEXT NOT NULL
                 );
                 CREATE INDEX entries_by_feed ON entries(feed_url, position);
                 CREATE TABLE entry_keys (
                   entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                   key      TEXT NOT NULL,
                   PRIMARY KEY (entry_id, key)
                 );
                 CREATE INDEX entry_keys_by_key ON entry_keys(key);
                 CREATE TABLE read_keys    (key TEXT PRIMARY KEY);
                 CREATE TABLE starred_keys (key TEXT PRIMARY KEY);
                 CREATE TABLE prefs (name TEXT PRIMARY KEY, value TEXT NOT NULL);

                 CREATE VIRTUAL TABLE entries_fts USING fts5(
                   title, summary, content='entries', content_rowid='id'
                 );
                 CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
                   INSERT INTO entries_fts(rowid, title, summary)
                   VALUES (new.id, new.title, new.summary);
                 END;
                 CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
                   INSERT INTO entries_fts(entries_fts, rowid, title, summary)
                   VALUES ('delete', old.id, old.title, old.summary);
                 END;
                 CREATE TRIGGER entries_au AFTER UPDATE OF title, summary ON entries BEGIN
                   INSERT INTO entries_fts(entries_fts, rowid, title, summary)
                   VALUES ('delete', old.id, old.title, old.summary);
                   INSERT INTO entries_fts(rowid, title, summary)
                   VALUES (new.id, new.title, new.summary);
                 END;
                 COMMIT;",
            )
            .context("creating the schema")?;
    }
    if from < 2 {
        // The article renderer needs the markup as published; `summary` is the
        // stripped text, which is all version 1 kept. Existing rows get an
        // empty string and fill in on the next refresh — a reader is not owed
        // rich rendering of an article fetched before the feature existed.
        connection
            .execute_batch("ALTER TABLE entries ADD COLUMN content TEXT NOT NULL DEFAULT '';")
            .context("adding the content column")?;
    }
    if from < 3 {
        // Keyed by the entry's identifier rather than its row, so a fetched
        // article survives the feed being refreshed out from under it —
        // otherwise "fetched once" would mean "fetched once per refresh".
        connection
            .execute_batch(
                "CREATE TABLE articles (
                   key        TEXT PRIMARY KEY,
                   html       TEXT NOT NULL,
                   fetched_at TEXT NOT NULL
                 );",
            )
            .context("adding the articles table")?;
    }
    if from < 4 {
        // When each feed was last actually reached, so a timer knows what is
        // due. Absent means never, which is due immediately.
        connection
            .execute_batch("ALTER TABLE feeds ADD COLUMN fetched_at TEXT;")
            .context("adding the fetched_at column")?;
    }
    if from < 5 {
        // Where a feed was actually found, when the configured address turned
        // out to be a web page that named one. Absent means "look where the
        // config says", which is the ordinary case.
        connection
            .execute_batch("ALTER TABLE feeds ADD COLUMN resolved_url TEXT;")
            .context("adding the resolved_url column")?;
    }
    if from < 6 {
        // A fingerprint of what was stored, so a refresh can tell an entry it
        // already has from one that has changed without reading every body
        // back out. Null on migrated rows, which reads as "unknown", so the
        // first refresh after upgrading rewrites them once and then settles.
        connection
            .execute_batch("ALTER TABLE entries ADD COLUMN digest INTEGER;")
            .context("adding the digest column")?;
        // The full-text index holds the title and the summary. Firing it on
        // every update meant moving an entry down the list re-indexed it, so
        // one new entry at the top re-indexed the whole feed.
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS entries_au;
                 CREATE TRIGGER entries_au AFTER UPDATE OF title, summary ON entries BEGIN
                   INSERT INTO entries_fts(entries_fts, rowid, title, summary)
                   VALUES ('delete', old.id, old.title, old.summary);
                   INSERT INTO entries_fts(rowid, title, summary)
                   VALUES (new.id, new.title, new.summary);
                 END;",
            )
            .context("narrowing the full-text trigger")?;
    }
    connection
        .pragma_update(None, "user_version", SCHEMA)
        .context("recording the schema version")?;
    Ok(())
}

/// Brings the old TOML files into the database, once.
///
/// Read state is the reader's own history and is not reproducible, so it is
/// carried across rather than discarded — `docs/stability.md` requires an older
/// format to be migrated forward, and a file is a format. The TOML is left on
/// disk untouched: deleting someone's data to tidy up is not our call.
pub fn migrate_from_toml(db: &mut Db, cache: &Path, state: &Path) -> Result<bool> {
    if db.flag("migrated_from_toml") {
        return Ok(false);
    }

    let mut brought_anything = false;

    if state.exists() {
        let old = crate::state::ReadState::load(state);
        let (read, starred) = old.keys_and_stars();
        if !read.is_empty() || !starred.is_empty() {
            db.save_state(&read, &HashSet::new(), &starred, &HashSet::new())?;
            brought_anything = true;
        }
        db.set_flag("unread_only", old.unread_only);
        db.set_flag("oldest_first", old.oldest_first);
    }

    if cache.exists() {
        for feed in crate::cache::Cache::load(cache).feeds() {
            db.put_feed(&feed)?;
            brought_anything = true;
        }
    }

    db.set_flag("migrated_from_toml", true);
    db.migrated_this_run = true;
    Ok(brought_anything)
}

/// `$XDG_DATA_HOME/rsst/rsst.sqlite3`, or the platform equivalent — or, if
/// `RSST_HOME` is set, that directory.
pub fn db_path() -> Result<PathBuf> {
    Ok(crate::home::dir(crate::home::Kind::Data)?.join("rsst.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, keys: &[&str]) -> Entry {
        Entry {
            title: title.into(),
            link: Some(format!("https://example.com/{title}")),
            published: None,
            summary: format!("The body of {title}, with words in it."),
            content: String::new(),
            keys: keys.iter().map(|k| (*k).to_string()).collect(),
        }
    }

    fn feed(url: &str, entries: Vec<Entry>) -> Feed {
        Feed {
            title: "Feed".into(),
            url: url.into(),
            entries,
            status: Status::Idle,
        }
    }

    fn source(url: &str) -> FeedSource {
        FeedSource {
            url: url.into(),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        }
    }

    fn db() -> Db {
        Db::in_memory().expect("an in-memory database")
    }

    #[test]
    fn a_stored_feed_comes_back_with_its_entries_and_keys() {
        let mut db = db();
        db.put_feed(&feed(
            "https://a.example",
            vec![entry("One", &["id:1", "link:1"])],
        ))
        .expect("put");

        let restored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("cached");
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.entries[0].title, "One");
        assert_eq!(restored.entries[0].keys, ["id:1", "link:1"]);
    }

    #[test]
    fn a_restored_feed_is_marked_fetching_because_a_refresh_is_coming() {
        let mut db = db();
        db.put_feed(&feed("https://a.example", vec![entry("One", &["id:1"])]))
            .expect("put");
        let restored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("cached");
        assert_eq!(restored.status, Status::Fetching);
    }

    #[test]
    fn the_configured_title_overrides_the_stored_one() {
        let mut db = db();
        db.put_feed(&feed("https://a.example", vec![]))
            .expect("put");
        let mut source = source("https://a.example");
        source.title = Some("Mine".into());
        assert_eq!(
            db.feed(&source).expect("query").expect("cached").title,
            "Mine"
        );
    }

    #[test]
    fn an_unknown_feed_is_absent() {
        assert!(
            db().feed(&source("https://nope.example"))
                .expect("query")
                .is_none()
        );
    }

    #[test]
    fn refreshing_one_feed_leaves_the_others_untouched() {
        // The whole point of the database: a refresh writes its own rows.
        let mut db = db();
        db.put_feed(&feed("https://a.example", vec![entry("A1", &["id:a1"])]))
            .expect("put");
        db.put_feed(&feed("https://b.example", vec![entry("B1", &["id:b1"])]))
            .expect("put");

        db.put_feed(&feed("https://a.example", vec![entry("A2", &["id:a2"])]))
            .expect("put");

        let b = db
            .feed(&source("https://b.example"))
            .expect("query")
            .expect("cached");
        assert_eq!(b.entries[0].title, "B1", "the other feed was rewritten");
        let a = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("cached");
        assert_eq!(a.entries.len(), 1);
        assert_eq!(a.entries[0].title, "A2");
    }

    #[test]
    fn a_starred_entry_survives_falling_out_of_the_feed() {
        let mut db = db();
        let original = entry("Kept", &["id:kept"]);
        db.put_feed(&feed(
            "https://a.example",
            vec![original.clone(), entry("Gone", &["id:gone"])],
        ))
        .expect("put");
        db.save_state(
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::from(["id:kept".to_string()]),
            &HashSet::new(),
        )
        .expect("star");
        db.starred.insert("id:kept".into());

        // The publisher rolls the window: only a new entry remains.
        db.put_feed(&feed("https://a.example", vec![entry("New", &["id:new"])]))
            .expect("put");

        let restored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("cached");
        let titles: Vec<_> = restored.entries.iter().map(|e| e.title.as_str()).collect();
        assert!(titles.contains(&"New"));
        assert!(titles.contains(&"Kept"), "the starred entry was dropped");
        assert!(!titles.contains(&"Gone"), "unstarred entries rolled away");
    }

    #[test]
    fn removing_a_feed_takes_its_entries_with_it() {
        let mut db = db();
        db.put_feed(&feed("https://a.example", vec![entry("A1", &["id:a1"])]))
            .expect("put");
        db.put_feed(&feed("https://b.example", vec![entry("B1", &["id:b1"])]))
            .expect("put");

        db.retain_configured(&[source("https://a.example")])
            .expect("retain");
        assert!(
            db.feed(&source("https://b.example"))
                .expect("query")
                .is_none()
        );
        assert_eq!(db.count("entries"), 1, "the orphaned entries went too");
    }

    #[test]
    fn read_keys_are_pruned_when_nothing_holds_them() {
        let mut db = db();
        db.put_feed(&feed("https://a.example", vec![entry("A1", &["id:a1"])]))
            .expect("put");
        db.save_state(
            &HashSet::from(["id:a1".to_string(), "id:long-gone".to_string()]),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        )
        .expect("save");

        assert_eq!(db.prune().expect("prune"), 1, "the orphan went");
        assert_eq!(db.count("read_keys"), 1, "the live one stayed");
    }

    #[test]
    fn the_run_that_migrates_does_not_prune_what_it_just_brought_across() {
        // The old file holds read keys for entries that rolled out of their
        // feeds long ago. Pruning on the same run would discard exactly the
        // history the migration went to the trouble of saving.
        let dir = std::env::temp_dir().join(format!("rsst-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let state = dir.join("read.toml");
        let cache = dir.join("feeds.toml");
        std::fs::write(
            &state,
            "version = 1\nread = [\"id:long-gone\"]\nstarred = []\n",
        )
        .expect("write");

        let path = dir.join("migrated.sqlite3");
        let _ = std::fs::remove_file(&path);
        let mut db = Db::open(&path).expect("open");
        migrate_from_toml(&mut db, &cache, &state).expect("migrate");

        assert_eq!(db.prune().expect("prune"), 0, "pruned on the migrating run");
        assert_eq!(db.count("read_keys"), 1, "the migrated key survived");

        // A later run prunes normally.
        let mut later = Db::open(&path).expect("reopen");
        assert_eq!(later.prune().expect("prune"), 1);
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let dir = std::env::temp_dir().join(format!("rsst-mig2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let state = dir.join("read.toml");
        std::fs::write(&state, "version = 1\nread = [\"id:a\"]\n").expect("write");
        let path = dir.join("twice.sqlite3");
        let _ = std::fs::remove_file(&path);

        let mut db = Db::open(&path).expect("open");
        assert!(migrate_from_toml(&mut db, &dir.join("nope.toml"), &state).expect("first"));
        assert!(
            !migrate_from_toml(&mut db, &dir.join("nope.toml"), &state).expect("second"),
            "the second migration did work again"
        );
    }

    #[test]
    fn a_starred_key_is_never_pruned_even_with_no_entry() {
        let mut db = db();
        db.save_state(
            &HashSet::from(["id:x".to_string()]),
            &HashSet::new(),
            &HashSet::from(["id:x".to_string()]),
            &HashSet::new(),
        )
        .expect("save");
        assert_eq!(db.prune().expect("prune"), 0);
    }

    #[test]
    fn state_is_written_as_a_delta_not_a_rewrite() {
        let mut db = db();
        db.save_state(
            &HashSet::from(["a".to_string(), "b".to_string()]),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        )
        .expect("add");
        db.save_state(
            &HashSet::new(),
            &HashSet::from(["a".to_string()]),
            &HashSet::new(),
            &HashSet::new(),
        )
        .expect("remove");

        let (read, _) = db.load_state().expect("load");
        assert_eq!(read, HashSet::from(["b".to_string()]));
    }

    #[test]
    fn validators_round_trip_and_a_missing_one_is_not_erased() {
        let db = db();
        db.set_validators("u", Some("\"abc\"".into()), Some("Mon".into()))
            .expect("set");
        db.set_validators("u", None, None).expect("set");

        let meta = db.meta("u");
        assert_eq!(meta.etag.as_deref(), Some("\"abc\""));
        assert_eq!(meta.last_modified.as_deref(), Some("Mon"));
    }

    #[test]
    fn a_deferred_feed_may_not_be_fetched_until_its_time() {
        let db = db();
        let now = Utc::now();
        db.defer_until("u", now + chrono::Duration::seconds(60))
            .expect("defer");
        assert!(!db.may_fetch("u", now));
        assert!(db.may_fetch("u", now + chrono::Duration::seconds(61)));
    }

    #[test]
    fn a_successful_response_clears_a_deferral() {
        let db = db();
        let now = Utc::now();
        db.defer_until("u", now + chrono::Duration::seconds(60))
            .expect("defer");
        db.set_validators("u", Some("\"x\"".into()), None)
            .expect("set");
        assert!(db.may_fetch("u", now));
    }

    #[test]
    fn full_text_search_finds_entries_across_feeds() {
        let mut db = db();
        db.put_feed(&feed(
            "https://a.example",
            vec![entry("Rust release notes", &["id:1"])],
        ))
        .expect("put");
        db.put_feed(&feed(
            "https://b.example",
            vec![entry("Rusty pipes", &["id:2"])],
        ))
        .expect("put");
        db.put_feed(&feed(
            "https://c.example",
            vec![entry("Cooking", &["id:3"])],
        ))
        .expect("put");

        let hits = db.search("rust", 50).expect("search");
        let urls: Vec<&str> = hits.iter().map(|(url, _)| url.as_str()).collect();
        assert!(urls.contains(&"https://a.example"));
        assert!(urls.contains(&"https://b.example"), "prefix match");
        assert!(!urls.contains(&"https://c.example"));
    }

    #[test]
    fn search_is_case_insensitive_and_looks_in_the_body() {
        let mut db = db();
        db.put_feed(&feed(
            "https://a.example",
            vec![entry("Anything", &["id:1"])],
        ))
        .expect("put");
        assert_eq!(db.search("ANYTHING", 10).expect("search").len(), 1);
        // The summary is "The body of Anything, with words in it."
        assert_eq!(db.search("words", 10).expect("search").len(), 1);
    }

    #[test]
    fn punctuation_in_a_query_is_not_read_as_syntax() {
        // A search for `c++` should find nothing, not raise a syntax error.
        let db = db();
        assert!(db.search("c++", 10).is_ok());
        assert!(db.search("\"unbalanced", 10).is_ok());
        assert!(db.search("a OR b AND (c", 10).is_ok());
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(db().search("   ", 10).expect("search").is_empty());
    }

    #[test]
    fn deleting_an_entry_removes_it_from_the_index() {
        let mut db = db();
        db.put_feed(&feed(
            "https://a.example",
            vec![entry("Findable", &["id:1"])],
        ))
        .expect("put");
        assert_eq!(db.search("Findable", 10).expect("search").len(), 1);

        db.put_feed(&feed("https://a.example", vec![entry("Other", &["id:2"])]))
            .expect("put");
        assert!(
            db.search("Findable", 10).expect("search").is_empty(),
            "the index still holds a deleted entry"
        );
    }

    #[test]
    fn flags_round_trip() {
        let db = db();
        assert!(!db.flag("unread_only"));
        db.set_flag("unread_only", true);
        assert!(db.flag("unread_only"));
        db.set_flag("unread_only", false);
        assert!(!db.flag("unread_only"));
    }

    #[test]
    fn a_feed_never_fetched_is_due_at_once() {
        let db = db();
        assert!(db.due("https://a.example", Duration::from_secs(1800), Utc::now()));
    }

    #[test]
    fn a_feed_just_fetched_is_not_due_again_yet() {
        let db = db();
        let now = Utc::now();
        db.mark_fetched("https://a.example", now).expect("mark");

        assert!(!db.due("https://a.example", Duration::from_secs(1800), now));
        assert!(db.due(
            "https://a.example",
            Duration::from_secs(1800),
            now + chrono::Duration::minutes(31)
        ));
    }

    #[test]
    fn a_zero_interval_turns_the_timer_off() {
        let db = db();
        assert!(!db.due("https://a.example", Duration::ZERO, Utc::now()));
    }

    #[test]
    fn a_rate_limited_feed_is_never_due() {
        // The back-off from 0016 outranks the timer; otherwise the timer would
        // hammer exactly the server that asked us to stop.
        let db = db();
        let now = Utc::now();
        db.defer_until("https://a.example", now + chrono::Duration::hours(1))
            .expect("defer");
        assert!(!db.due("https://a.example", Duration::from_secs(60), now));
    }

    #[test]
    fn a_not_modified_response_still_counts_as_reached() {
        // Otherwise a feed that never changes would be asked on every tick.
        let db = db();
        let now = Utc::now();
        db.mark_fetched("https://a.example", now).expect("mark");
        assert!(!db.due("https://a.example", Duration::from_secs(600), now));
    }

    #[test]
    fn a_fetched_article_is_found_by_any_of_the_entry_keys() {
        let db = db();
        let original = entry("One", &["id:1", "link:1"]);
        assert!(db.article(&original).is_none());

        db.put_article(&original, "<p>Full text</p>").expect("put");
        assert_eq!(
            db.article(&entry("One", &["id:1"])).as_deref(),
            Some("<p>Full text</p>")
        );
        // A feed that regenerated its guid still finds it by the link.
        assert_eq!(
            db.article(&entry("One", &["id:regenerated", "link:1"]))
                .as_deref(),
            Some("<p>Full text</p>")
        );
    }

    #[test]
    fn a_fetched_article_survives_the_feed_being_refreshed() {
        // Otherwise "fetched once" would mean "once per refresh".
        let mut db = db();
        let one = entry("One", &["id:1"]);
        db.put_feed(&feed("https://a.example", vec![one.clone()]))
            .expect("put");
        db.put_article(&one, "<p>Full</p>").expect("put");

        db.put_feed(&feed("https://a.example", vec![entry("Two", &["id:2"])]))
            .expect("refresh");
        assert_eq!(db.article(&one).as_deref(), Some("<p>Full</p>"));
    }

    #[test]
    fn fetching_again_replaces_what_was_stored() {
        let db = db();
        let one = entry("One", &["id:1"]);
        db.put_article(&one, "<p>Old</p>").expect("put");
        db.put_article(&one, "<p>New</p>").expect("put");
        assert_eq!(db.article(&one).as_deref(), Some("<p>New</p>"));
    }

    #[test]
    fn a_version_one_database_gains_the_content_column() {
        // The migration path `docs/stability.md` promises, exercised for real.
        let dir = std::env::temp_dir().join(format!("rsst-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("v1.sqlite3");
        let _ = std::fs::remove_file(&path);

        {
            // A version 1 schema, without `content`.
            let connection = Connection::open(&path).expect("open");
            connection
                .execute_batch(
                    "CREATE TABLE feeds (url TEXT PRIMARY KEY, title TEXT NOT NULL,
                       etag TEXT, last_modified TEXT, retry_after TEXT);
                     CREATE TABLE entries (id INTEGER PRIMARY KEY,
                       feed_url TEXT NOT NULL REFERENCES feeds(url) ON DELETE CASCADE,
                       position INTEGER NOT NULL, title TEXT NOT NULL, link TEXT,
                       published TEXT, summary TEXT NOT NULL);
                     CREATE TABLE entry_keys (entry_id INTEGER NOT NULL, key TEXT NOT NULL,
                       PRIMARY KEY (entry_id, key));
                     CREATE TABLE read_keys (key TEXT PRIMARY KEY);
                     CREATE TABLE starred_keys (key TEXT PRIMARY KEY);
                     CREATE TABLE prefs (name TEXT PRIMARY KEY, value TEXT NOT NULL);
                     CREATE VIRTUAL TABLE entries_fts USING fts5(title, summary,
                       content='entries', content_rowid='id');
                     INSERT INTO feeds (url, title) VALUES ('https://a.example', 'A');
                     INSERT INTO entries (feed_url, position, title, summary)
                       VALUES ('https://a.example', 0, 'Old entry', 'Body');",
                )
                .expect("v1 schema");
            connection
                .pragma_update(None, "user_version", 1)
                .expect("version");
        }

        let db = Db::open(&path).expect("migrates");
        let feed = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        assert_eq!(feed.entries.len(), 1, "the old row survived");
        assert_eq!(feed.entries[0].title, "Old entry");
        assert_eq!(feed.entries[0].content, "", "no markup was invented");
    }

    #[test]
    fn a_version_four_database_gains_the_resolved_url_column() {
        // Same promise as the version one test, one schema later: read state
        // is the reader's own history and must survive the upgrade.
        let dir = std::env::temp_dir().join(format!("rsst-v4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("v4.sqlite3");
        let _ = std::fs::remove_file(&path);

        {
            let connection = Connection::open(&path).expect("open");
            // Built by the real migrations, then stopped one short — so this
            // is the schema version 4 actually shipped, not a guess at it.
            migrate(&connection, 0).expect("build up to current");
            connection
                .execute_batch(
                    "ALTER TABLE feeds DROP COLUMN resolved_url;
                     ALTER TABLE entries DROP COLUMN digest;
                     INSERT INTO feeds (url, title) VALUES ('https://a.example', 'A');
                     INSERT INTO entries (feed_url, position, title, summary)
                       VALUES ('https://a.example', 0, 'Old entry', 'Body');
                     INSERT INTO read_keys (key) VALUES ('id:kept');",
                )
                .expect("v4 shape");
            connection
                .pragma_update(None, "user_version", 4)
                .expect("version");
        }

        let db = Db::open(&path).expect("migrates");
        let feed = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        assert_eq!(feed.entries.len(), 1, "the old row survived");
        assert!(
            db.load_state().expect("state").0.contains("id:kept"),
            "read state did not survive the migration"
        );
        assert_eq!(
            db.resolved("https://a.example"),
            None,
            "a migrated feed has not discovered anything yet"
        );

        db.set_resolved("https://a.example", Some("https://a.example/atom.xml"))
            .expect("record");
        assert_eq!(
            db.resolved("https://a.example").as_deref(),
            Some("https://a.example/atom.xml")
        );
    }

    #[test]
    fn a_version_five_database_gains_the_digest_column() {
        let dir = std::env::temp_dir().join(format!("rsst-v5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("v5.sqlite3");
        let _ = std::fs::remove_file(&path);

        {
            let connection = Connection::open(&path).expect("open");
            migrate(&connection, 0).expect("build up to current");
            connection
                .execute_batch(
                    "ALTER TABLE entries DROP COLUMN digest;
                     INSERT INTO feeds (url, title) VALUES ('https://a.example', 'A');
                     INSERT INTO entries (feed_url, position, title, summary)
                       VALUES ('https://a.example', 0, 'Old entry', 'Body');
                     INSERT INTO read_keys (key) VALUES ('id:kept');",
                )
                .expect("v5 shape");
            connection
                .pragma_update(None, "user_version", 5)
                .expect("version");
        }

        let db = Db::open(&path).expect("migrates");
        let feed = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        assert_eq!(feed.entries.len(), 1, "the old row survived");
        assert_eq!(feed.entries[0].title, "Old entry");
        assert!(
            db.load_state().expect("state").0.contains("id:kept"),
            "read state did not survive the migration"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_database_from_a_newer_rsst_is_refused_rather_than_misread() {
        let connection = Connection::open_in_memory().expect("open");
        connection
            .pragma_update(None, "user_version", SCHEMA + 1)
            .expect("set version");
        let err = Db::from_connection(connection).expect_err("refused");
        assert!(err.to_string().contains("understands"));
    }

    #[test]
    fn opening_twice_does_not_migrate_twice() {
        let dir = std::env::temp_dir().join(format!("rsst-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("twice.sqlite3");
        let _ = std::fs::remove_file(&path);

        {
            let mut db = Db::open(&path).expect("open");
            db.put_feed(&feed("https://a.example", vec![entry("One", &["id:1"])]))
                .expect("put");
        }
        let db = Db::open(&path).expect("reopen");
        assert_eq!(db.count("entries"), 1);
        assert_eq!(db.count("feeds"), 1);
    }

    #[test]
    fn a_refresh_that_changes_nothing_writes_nothing() {
        // The whole point: `put_feed` used to delete and re-insert every row
        // whatever had changed, which cost 654 ms on a five-thousand-entry
        // feed and froze the interface while it ran.
        let mut db = Db::in_memory().expect("db");
        let feed = feed(
            "https://a.example",
            (0..200)
                .map(|n| entry(&format!("Entry {n}"), &[&format!("id:{n}")]))
                .collect(),
        );

        db.put_feed(&feed).expect("first store");
        assert_eq!(db.last_written(), 200, "the first store writes them all");

        db.put_feed(&feed).expect("second store");
        assert_eq!(db.last_written(), 0, "an unchanged refresh wrote rows");
    }

    #[test]
    fn only_the_entry_that_changed_is_rewritten() {
        let mut db = Db::in_memory().expect("db");
        let mut feed = feed(
            "https://a.example",
            (0..50)
                .map(|n| entry(&format!("Entry {n}"), &[&format!("id:{n}")]))
                .collect(),
        );
        db.put_feed(&feed).expect("store");

        // The publisher fixed a typo in one headline.
        feed.entries[7].title = "Entry 7, corrected".into();
        db.put_feed(&feed).expect("store");
        assert_eq!(db.last_written(), 1, "one edit rewrote more than one row");

        let stored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        assert_eq!(stored.entries[7].title, "Entry 7, corrected");
        assert_eq!(stored.entries.len(), 50, "nothing else was disturbed");
    }

    #[test]
    fn a_new_entry_arrives_at_the_top_and_the_rest_keep_their_order() {
        let mut db = Db::in_memory().expect("db");
        let mut feed = feed(
            "https://a.example",
            (0..5)
                .map(|n| entry(&format!("Entry {n}"), &[&format!("id:{n}")]))
                .collect(),
        );
        db.put_feed(&feed).expect("store");

        feed.entries.insert(0, entry("Newest", &["id:new"]));
        db.put_feed(&feed).expect("store");

        let stored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        let titles: Vec<&str> = stored.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Newest", "Entry 0", "Entry 1", "Entry 2", "Entry 3", "Entry 4"
            ]
        );
    }

    #[test]
    fn a_dropped_entry_goes_unless_it_was_starred() {
        let mut db = Db::in_memory().expect("db");
        let mut feed = feed(
            "https://a.example",
            vec![
                entry("Kept", &["id:kept"]),
                entry("Starred", &["id:starred"]),
                entry("Dropped", &["id:dropped"]),
            ],
        );
        db.put_feed(&feed).expect("store");
        db.save_state(
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::from(["id:starred".to_string()]),
            &HashSet::new(),
        )
        .expect("star");

        // The publisher drops the last two.
        feed.entries.truncate(1);
        db.put_feed(&feed).expect("store");

        let stored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        let titles: Vec<&str> = stored.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Kept", "Starred"],
            "a starred entry must survive the publisher dropping it, below what is still published"
        );
    }

    #[test]
    fn read_state_follows_an_entry_that_was_rewritten() {
        // Rewriting in place rather than delete-and-insert is what keeps the
        // keys, and read state is matched on keys.
        let mut db = Db::in_memory().expect("db");
        let mut feed = feed("https://a.example", vec![entry("One", &["id:1"])]);
        db.put_feed(&feed).expect("store");

        feed.entries[0].summary = "Rewritten body.".into();
        db.put_feed(&feed).expect("store");

        let stored = db
            .feed(&source("https://a.example"))
            .expect("query")
            .expect("kept");
        assert_eq!(stored.entries[0].keys, vec!["id:1".to_string()]);
        assert_eq!(stored.entries[0].summary, "Rewritten body.");
    }

    #[test]
    fn moving_an_entry_does_not_disturb_the_search_index() {
        // The index holds the title and the summary. A row that only moved has
        // not changed either, and re-indexing it is how one new entry used to
        // re-index a whole feed.
        let mut db = Db::in_memory().expect("db");
        let mut feed = feed(
            "https://a.example",
            vec![
                entry("Alpha", &["id:a"]),
                entry("Bravo", &["id:b"]),
                entry("Charlie", &["id:c"]),
            ],
        );
        db.put_feed(&feed).expect("store");

        feed.entries.insert(0, entry("Delta", &["id:d"]));
        db.put_feed(&feed).expect("store");

        // Everything is still findable, in both directions.
        for word in ["Alpha", "Bravo", "Charlie", "Delta"] {
            assert!(
                !db.search(word, 10).expect("search").is_empty(),
                "{word} fell out of the index when the list moved"
            );
        }
    }
}
