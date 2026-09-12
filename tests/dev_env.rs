//! The dev environment must be a dead end: nothing it does may reach real data.
//!
//! `scripts/dev` is shell, so what is tested here is the promises it rests on —
//! that the seeder writes only inside the home it is given, and that it needs
//! one. A script that forgets `RSST_HOME` would otherwise seed the reader you
//! actually use, which is the one mistake this whole thing exists to prevent.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary(name: &str) -> PathBuf {
    let mut path = std::env::current_exe().expect("the test binary has a path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("rsst-dev-{name}-{}", std::process::id()));
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

#[test]
fn seeding_without_a_home_refuses_rather_than_guessing() {
    // No --home and no RSST_HOME. The dangerous behaviour would be to fall
    // back to the platform's directory and seed the real reader.
    let output = Command::new(binary("rsst-seed"))
        .env_remove(rsst::home::HOME)
        .output()
        .expect("rsst-seed runs");

    assert!(
        !output.status.success(),
        "seeding with no home was allowed; it would have written to real data"
    );
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains("--home") || said.contains(rsst::home::HOME),
        "the refusal does not say what is missing: {said:?}"
    );
}

#[test]
fn seeding_writes_a_config_only_inside_the_home_it_was_given() {
    let scratch = Scratch::new("seed");
    let real = rsst::config::config_path().expect("a platform config path");
    let before = std::fs::metadata(&real).ok().map(|m| m.modified().ok());

    // Port 1 answers nothing, so every feed fails. Seeding must still write
    // the config and the database — a dead fixture server is not a reason to
    // leave a half-made environment behind.
    let output = Command::new(binary("rsst-seed"))
        // `--offline` because a test must not depend on the internet, and
        // without it the seeder would reach for ten real feeds.
        .args([
            "--home",
            &scratch.path().display().to_string(),
            "--port",
            "1",
            "--offline",
        ])
        .output()
        .expect("rsst-seed runs");
    assert!(
        output.status.success(),
        "seeding failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(scratch.path().join("config.toml").is_file(), "no config");
    assert!(scratch.path().join("rsst.sqlite3").is_file(), "no database");

    let after = std::fs::metadata(&real).ok().map(|m| m.modified().ok());
    assert_eq!(
        before,
        after,
        "seeding touched the real config at {}",
        real.display()
    );
}

#[test]
fn the_fixtures_are_the_ones_the_seeder_asks_for() {
    // The seeder names files; the files have to exist, or the environment is
    // quietly four feeds smaller than it looks.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/feeds");
    for name in ["handbook.xml", "unicode.xml", "awkward.xml", "empty.xml"] {
        assert!(root.join(name).is_file(), "missing fixture {name}");
    }
}

#[test]
fn seeding_survives_every_single_feed_failing() {
    // The state a test machine is usually in: nothing reachable at all.
    let scratch = Scratch::new("alldead");
    let output = Command::new(binary("rsst-seed"))
        .args([
            "--home",
            &scratch.path().display().to_string(),
            "--port",
            "1",
            "--offline",
        ])
        .output()
        .expect("rsst-seed runs");

    assert!(
        output.status.success(),
        "seeding gave up when the feeds did"
    );
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains("unavailable"),
        "it did not say which feeds it could not reach: {said}"
    );
    assert!(
        scratch.path().join("rsst.sqlite3").is_file(),
        "no database, so the reader would have nothing to open"
    );
}

#[test]
fn an_offline_seed_configures_nothing_off_this_machine() {
    // A real feed sneaking into the offline set is how a deterministic frame
    // quietly stops being deterministic.
    let scratch = Scratch::new("offline");
    Command::new(binary("rsst-seed"))
        .args([
            "--home",
            &scratch.path().display().to_string(),
            "--port",
            "1",
            "--offline",
        ])
        .output()
        .expect("rsst-seed runs");

    let config = std::fs::read_to_string(scratch.path().join("config.toml")).expect("a config");
    for line in config.lines().filter(|line| line.starts_with("url =")) {
        assert!(
            line.contains("127.0.0.1"),
            "offline seeding configured a feed off this machine: {line}"
        );
    }
}
