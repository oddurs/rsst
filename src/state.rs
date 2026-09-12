use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::feed::Entry;

/// Which entries have been read, persisted between runs.
///
/// Entries are identified by a *set* of candidate keys rather than one, because
/// feeds are not disciplined about their identifiers: a publisher that
/// regenerates guids on every build would otherwise resurrect the whole feed as
/// unread. An entry counts as read if any of its candidates is stored, and
/// marking one stores all of them, so read state survives a feed changing its
/// mind about which identifier is canonical.
#[derive(Debug, Default)]
pub struct ReadState {
    keys: HashSet<String>,
    /// Entries the reader has starred, by the same candidate keys.
    starred: HashSet<String>,
    /// Whether the entry list is filtered to unread. Kept here because it is
    /// the same kind of thing — what the reader remembers between runs.
    pub unread_only: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct OnDisk {
    #[serde(default)]
    read: Vec<String>,
    #[serde(default)]
    starred: Vec<String>,
    #[serde(default)]
    unread_only: bool,
}

impl ReadState {
    /// Reads the state file, treating a missing or unreadable one as empty.
    ///
    /// Losing read state is a small annoyance; refusing to start over it is a
    /// large one, so a corrupt file is discarded rather than fatal.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        let parsed: OnDisk = toml::from_str(&raw).unwrap_or_default();
        Self {
            keys: parsed.read.into_iter().collect(),
            starred: parsed.starred.into_iter().collect(),
            unread_only: parsed.unread_only,
        }
    }

    /// Writes the state file atomically.
    ///
    /// Writes a sibling temporary file and renames it over the target, so a
    /// crash mid-write leaves the previous state intact rather than a truncated
    /// file. `rename` is atomic within a directory on every platform we target.
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("state path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("creating state directory {}", parent.display()))?;

        let mut read: Vec<&String> = self.keys.iter().collect();
        read.sort(); // stable on disk, so the file diffs cleanly and tests can compare
        let mut starred: Vec<&String> = self.starred.iter().collect();
        starred.sort();

        let body = toml::to_string_pretty(&OnDisk {
            read: read.into_iter().cloned().collect(),
            starred: starred.into_iter().cloned().collect(),
            unread_only: self.unread_only,
        })
        .context("serializing read state")?;

        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, path)
            .with_context(|| format!("replacing {} with {}", path.display(), tmp.display()))
    }

    pub fn is_read(&self, entry: &Entry) -> bool {
        entry.keys.iter().any(|key| self.keys.contains(key))
    }

    pub fn mark_read(&mut self, entry: &Entry) {
        self.keys.extend(entry.keys.iter().cloned());
    }

    /// Puts an entry back to unread.
    ///
    /// Every candidate key has to go: leaving one behind would leave the entry
    /// still matching, and the toggle would look broken.
    pub fn mark_unread(&mut self, entry: &Entry) {
        for key in &entry.keys {
            self.keys.remove(key);
        }
    }

    pub fn is_starred(&self, entry: &Entry) -> bool {
        entry.keys.iter().any(|key| self.starred.contains(key))
    }

    /// Stars an entry, or unstars it if it already was.
    pub fn toggle_star(&mut self, entry: &Entry) -> bool {
        if self.is_starred(entry) {
            for key in &entry.keys {
                self.starred.remove(key);
            }
            false
        } else {
            self.starred.extend(entry.keys.iter().cloned());
            true
        }
    }

    /// Whether any of these keys is starred.
    ///
    /// Takes raw keys rather than an entry so the cache can ask about entries
    /// it is about to drop.
    pub fn any_starred(&self, keys: &[String]) -> bool {
        keys.iter().any(|key| self.starred.contains(key))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.keys.len()
    }
}

/// `$XDG_DATA_HOME/rsst/read.toml`, or the platform equivalent.
pub fn state_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "rsst")
        .context("could not determine a data directory for this platform")?;
    Ok(dirs.data_dir().join("read.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(keys: &[&str]) -> Entry {
        Entry {
            title: "t".into(),
            link: None,
            published: None,
            summary: String::new(),
            keys: keys.iter().map(|k| (*k).to_string()).collect(),
        }
    }

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rsst-state-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn an_unmarked_entry_is_unread() {
        let state = ReadState::default();
        assert!(!state.is_read(&entry(&["a"])));
    }

    #[test]
    fn marking_an_entry_makes_it_read() {
        let mut state = ReadState::default();
        state.mark_read(&entry(&["a", "b"]));
        assert!(state.is_read(&entry(&["a", "b"])));
    }

    #[test]
    fn read_state_survives_a_changed_identifier() {
        // The feed used to publish guid "a" with link "b"; after a rebuild the
        // guid is "zzz" but the link is unchanged. It must stay read.
        let mut state = ReadState::default();
        state.mark_read(&entry(&["a", "b"]));
        assert!(state.is_read(&entry(&["zzz", "b"])));
    }

    #[test]
    fn an_entry_can_be_put_back_to_unread() {
        let mut state = ReadState::default();
        state.mark_read(&entry(&["a", "b"]));
        assert!(state.is_read(&entry(&["a", "b"])));

        state.mark_unread(&entry(&["a", "b"]));
        assert!(!state.is_read(&entry(&["a", "b"])));
        assert_eq!(state.len(), 0, "every candidate key was removed");
    }

    #[test]
    fn marking_unread_leaves_other_entries_alone() {
        let mut state = ReadState::default();
        state.mark_read(&entry(&["a"]));
        state.mark_read(&entry(&["b"]));
        state.mark_unread(&entry(&["a"]));
        assert!(!state.is_read(&entry(&["a"])));
        assert!(state.is_read(&entry(&["b"])));
    }

    #[test]
    fn an_entry_sharing_no_key_is_a_different_entry() {
        let mut state = ReadState::default();
        state.mark_read(&entry(&["a", "b"]));
        assert!(!state.is_read(&entry(&["c", "d"])));
    }

    #[test]
    fn state_round_trips_through_a_file() {
        let path = tmpdir().join("round-trip.toml");
        let mut state = ReadState::default();
        state.mark_read(&entry(&["a", "b"]));
        state.save(&path).expect("save");

        let loaded = ReadState::load(&path);
        assert!(loaded.is_read(&entry(&["a"])));
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn saving_leaves_no_temporary_file_behind() {
        let dir = tmpdir();
        let path = dir.join("clean.toml");
        ReadState::default().save(&path).expect("save");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary file was not renamed away");
    }

    #[test]
    fn the_unread_filter_survives_a_restart() {
        let path = tmpdir().join("filter.toml");
        let state = ReadState {
            unread_only: true,
            ..Default::default()
        };
        state.save(&path).expect("save");
        assert!(ReadState::load(&path).unread_only);
    }

    #[test]
    fn starring_toggles_and_is_independent_of_read_state() {
        let mut state = ReadState::default();
        assert!(state.toggle_star(&entry(&["a", "b"])));
        assert!(state.is_starred(&entry(&["a"])));
        assert!(!state.is_read(&entry(&["a"])), "starring is not reading");

        assert!(!state.toggle_star(&entry(&["a", "b"])));
        assert!(!state.is_starred(&entry(&["a"])));
    }

    #[test]
    fn stars_survive_a_restart() {
        let path = tmpdir().join("stars.toml");
        let mut state = ReadState::default();
        state.toggle_star(&entry(&["a", "b"]));
        state.save(&path).expect("save");
        assert!(ReadState::load(&path).is_starred(&entry(&["b"])));
    }

    #[test]
    fn raw_keys_can_be_checked_for_stars() {
        let mut state = ReadState::default();
        state.toggle_star(&entry(&["a"]));
        assert!(state.any_starred(&["a".to_string()]));
        assert!(!state.any_starred(&["z".to_string()]));
    }

    #[test]
    fn a_missing_file_loads_as_empty() {
        let state = ReadState::load(Path::new("/nonexistent/rsst/read.toml"));
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn a_corrupt_file_loads_as_empty_rather_than_failing() {
        let path = tmpdir().join("corrupt.toml");
        fs::write(&path, "this is not : valid toml [[[").expect("write");
        assert_eq!(ReadState::load(&path).len(), 0);
    }
}
