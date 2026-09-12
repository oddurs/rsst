//! Running rsst against a throwaway home, through the real binary.
//!
//! The unit tests in `src/home.rs` cover the decision; this covers the wiring.
//! Setting an environment variable is unsafe in this edition and process-global
//! besides, so the only honest way to test it is to set it on a child.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The binary under test, built alongside this test by cargo.
fn rsst() -> PathBuf {
    // `target/debug/deps/home-<hash>` — the binary is two levels up.
    let mut path = std::env::current_exe().expect("the test binary has a path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!("rsst{}", std::env::consts::EXE_SUFFIX))
}

/// A directory that is removed when the test ends, pass or fail.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("rsst-home-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs rsst with `RSST_HOME` set, and nothing else different.
fn run_in(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(rsst())
        .args(args)
        .env("RSST_HOME", home)
        .output()
        .expect("rsst runs")
}

#[test]
fn a_first_run_writes_its_config_into_the_home_it_was_given() {
    let scratch = Scratch::new("first");

    // What the platform would have used, had the home not been given. The
    // test must leave it exactly as it found it — that is the whole promise.
    let real = rsst::config::config_path().expect("a platform config path");
    let before = std::fs::metadata(&real)
        .ok()
        .map(|meta| meta.modified().ok());

    // `--screenshot` runs the whole startup and exits, so it needs no tty.
    let output = run_in(scratch.path(), &["--screenshot", "80x24"]);
    let said = String::from_utf8_lossy(&output.stderr);

    let config = scratch.path().join("config.toml");
    assert!(
        config.is_file(),
        "no config at {}; rsst said {said:?}",
        config.display()
    );

    let after = std::fs::metadata(&real)
        .ok()
        .map(|meta| meta.modified().ok());
    assert_eq!(
        before,
        after,
        "rsst wrote to the real config at {} despite being given a home",
        real.display()
    );
}

#[test]
fn the_database_lands_in_the_home_too() {
    let scratch = Scratch::new("db");
    // A config with a feed, so rsst gets past the "no feeds" exit and opens
    // the database. The feed need not resolve: a dead feed is a placeholder,
    // not a failure to start.
    std::fs::write(
        scratch.path().join("config.toml"),
        "[[feeds]]\nurl = \"http://127.0.0.1:1/never.xml\"\n",
    )
    .expect("writing a config");

    // `--screenshot` runs the whole startup — config, database, fetch — and
    // then exits, which is what makes it usable as a test harness.
    let output = run_in(scratch.path(), &["--screenshot", "80x24"]);
    assert!(
        output.status.success(),
        "rsst failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        scratch.path().join("rsst.sqlite3").is_file(),
        "no database in the home: {:?}",
        std::fs::read_dir(scratch.path()).map(|entries| entries
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect::<Vec<_>>())
    );
}

#[test]
fn two_homes_do_not_share_a_database() {
    let one = Scratch::new("one");
    let two = Scratch::new("two");
    for scratch in [&one, &two] {
        std::fs::write(
            scratch.path().join("config.toml"),
            "[[feeds]]\nurl = \"http://127.0.0.1:1/never.xml\"\n",
        )
        .expect("writing a config");
        run_in(scratch.path(), &["--screenshot", "80x24"]);
    }

    let size = |scratch: &Scratch| {
        std::fs::metadata(scratch.path().join("rsst.sqlite3"))
            .map(|meta| meta.len())
            .unwrap_or_default()
    };
    assert!(
        size(&one) > 0 && size(&two) > 0,
        "both homes get a database"
    );
    assert_ne!(
        one.path().join("rsst.sqlite3"),
        two.path().join("rsst.sqlite3"),
        "the two homes resolved to the same file"
    );
}
