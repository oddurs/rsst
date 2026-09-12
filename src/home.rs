//! Where rsst keeps its files.
//!
//! Normally the platform decides: a config directory and a data directory,
//! chosen by `directories`. Setting [`HOME`] overrides both at once and puts
//! everything under one directory instead.
//!
//! One variable rather than one per file, because the point is to run a second
//! rsst that cannot touch the first one's data. Two variables would let you set
//! one and forget the other, which is the failure this exists to prevent.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Set this to keep the config, the database and the read state under one
/// directory of your choosing.
pub const HOME: &str = "RSST_HOME";

/// Which of the platform's two directories a file belongs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Config,
    Data,
    Cache,
}

/// The directory a file of this kind lives in.
pub fn dir(kind: Kind) -> Result<PathBuf> {
    resolve(std::env::var_os(HOME), kind)
}

/// The same decision, over a value rather than the environment.
///
/// Separated so it can be tested: setting an environment variable is unsafe in
/// this edition, and process-global besides, which makes tests that do it race
/// every other test in the binary.
fn resolve(home: Option<OsString>, kind: Kind) -> Result<PathBuf> {
    // An empty value reads as "not set". Otherwise `RSST_HOME= rsst` would
    // quietly put the database in the working directory.
    if let Some(home) = home.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    let dirs = directories::ProjectDirs::from("", "", "rsst")
        .context("could not determine a config directory for this platform")?;
    Ok(match kind {
        Kind::Config => dirs.config_dir().to_path_buf(),
        Kind::Data => dirs.data_dir().to_path_buf(),
        Kind::Cache => dirs.cache_dir().to_path_buf(),
    })
}

/// Whether [`HOME`] is in force, for anything that wants to say so.
pub fn overridden() -> bool {
    std::env::var_os(HOME).is_some_and(|value| !value.is_empty())
}

/// Creates a directory and everything above it, naming it if that fails.
pub fn ensure(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_override_takes_both_directories() {
        let home = Some(OsString::from("/tmp/somewhere"));
        assert_eq!(
            resolve(home.clone(), Kind::Config).expect("resolves"),
            PathBuf::from("/tmp/somewhere")
        );
        assert_eq!(
            resolve(home, Kind::Data).expect("resolves"),
            PathBuf::from("/tmp/somewhere"),
            "config and data must land together, or one would be left behind"
        );
        assert_eq!(
            resolve(Some(OsString::from("/tmp/somewhere")), Kind::Cache).expect("resolves"),
            PathBuf::from("/tmp/somewhere"),
            "the cache too, or a fresh home would migrate the real one's feeds in"
        );
    }

    #[test]
    fn without_an_override_the_platform_decides() {
        // Not `ends_with`: Windows puts the data directory under `rsst\data`,
        // so the name is a component rather than the last one.
        for (kind, path) in [
            (Kind::Config, resolve(None, Kind::Config).expect("resolves")),
            (Kind::Data, resolve(None, Kind::Data).expect("resolves")),
        ] {
            assert!(
                path.components()
                    .any(|part| part.as_os_str().eq_ignore_ascii_case("rsst")),
                "{kind:?} resolved to {path:?}, which is not rsst's own"
            );
        }
    }

    #[test]
    fn an_empty_value_is_not_an_override() {
        // `RSST_HOME= rsst` would otherwise write the database into whatever
        // directory the shell happened to be in.
        let platform = resolve(None, Kind::Data).expect("resolves");
        assert_eq!(
            resolve(Some(OsString::new()), Kind::Data).expect("resolves"),
            platform
        );
    }
}
