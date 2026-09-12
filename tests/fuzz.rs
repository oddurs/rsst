//! Feeding the parser and the renderer things no publisher would send.
//!
//! `SECURITY.md` promises that a malicious feed cannot crash the reader, hang
//! it, or reach outside its data directory. That promise needs something to
//! back it, and for a long time had nothing.
//!
//! This is deterministic mutation fuzzing rather than coverage-guided fuzzing:
//! `cargo-fuzz` needs nightly, and a check that cannot run in CI is a check
//! that rots. Seeds are committed, mutations come from a fixed seed so a
//! failure is reproducible, and `RSST_FUZZ_ITERATIONS` turns it up for a
//! longer run by hand.

use std::time::{Duration, Instant};

use rsst::config::FeedSource;

/// No single input may take longer than this.
///
/// Generous — CI runners are slow and shared. It is here to catch an input
/// that never finishes, not one that is merely slow.
const BUDGET: Duration = Duration::from_secs(2);

fn seeds() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .expect("the corpus directory must exist")
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, std::fs::read(entry.path()).expect("readable seed"))
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "the corpus is empty");
    out
}

fn source() -> FeedSource {
    FeedSource {
        url: "https://fuzz.example/feed".into(),
        refresh_minutes: None,
        title: None,
        tags: Vec::new(),
    }
}

/// Runs one input through everything that touches untrusted bytes.
fn exercise(bytes: &[u8]) {
    if let Ok(feed) = rsst::feed::parse(bytes, &source()) {
        for entry in feed.entries.iter().take(20) {
            // The article renderer sees whatever the feed put in the body.
            let article = rsst::article::parse(&entry.content);
            for width in [1usize, 17, 80] {
                let _ = rsst::article::layout(&article, width, false);
            }
            let _ = rsst::readable::extract(&entry.content);
            let _ = rsst::feed::to_plain_text(&entry.content);
        }
    }
    // The renderer and extractor must also survive bytes that are not a feed.
    let text = String::from_utf8_lossy(bytes);
    let _ = rsst::article::layout(&rsst::article::parse(&text), 40, false);
    let _ = rsst::readable::extract(&text);
}

/// A tiny deterministic generator, so a failure can be reproduced exactly.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

/// Damages an input in one of the ways the real world does.
fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    if bytes.is_empty() {
        return bytes;
    }
    match rng.below(6) {
        // Cut short, as a dropped connection does.
        0 => bytes.truncate(rng.below(bytes.len())),
        // Flip a byte, as corruption does.
        1 => {
            let at = rng.below(bytes.len());
            bytes[at] = (rng.next() & 0xff) as u8;
        }
        // Repeat a chunk, as a bad proxy does.
        2 => {
            let at = rng.below(bytes.len());
            let take = rng.below(bytes.len() - at).min(512);
            let chunk = bytes[at..at + take].to_vec();
            bytes.splice(at..at, chunk);
        }
        // Inject markup where markup was not expected.
        3 => {
            let at = rng.below(bytes.len());
            let junk = [
                b"<div>".as_slice(),
                b"</p>".as_slice(),
                b"<a href=\"".as_slice(),
                b"&#x".as_slice(),
                b"<![CDATA[".as_slice(),
                b"<pre>".as_slice(),
            ][rng.below(6)];
            bytes.splice(at..at, junk.iter().copied());
        }
        // Nest deeply, to find anything recursive.
        4 => {
            let at = rng.below(bytes.len());
            let depth = 1 + rng.below(200);
            let nest: Vec<u8> = b"<div>".repeat(depth);
            bytes.splice(at..at, nest);
        }
        // Splice two seeds together.
        _ => bytes.extend_from_slice(&seed[..rng.below(seed.len())]),
    }
    bytes
}

#[test]
fn every_seed_in_the_corpus_is_survivable() {
    for (name, bytes) in seeds() {
        let started = Instant::now();
        exercise(&bytes);
        assert!(
            started.elapsed() < BUDGET,
            "{name} took {:?}, which is long enough to look like a hang",
            started.elapsed()
        );
    }
}

#[test]
fn mutations_of_the_corpus_are_survivable() {
    let seeds = seeds();
    let iterations: usize = std::env::var("RSST_FUZZ_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2_000);

    // Fixed, so a failure here reproduces exactly rather than once in a while.
    let mut rng = Rng(0x5DEE_CE66_D1CE_F00D);

    for iteration in 0..iterations {
        let (name, seed) = &seeds[rng.below(seeds.len())];
        let input = mutate(&mut rng, seed);
        let started = Instant::now();
        exercise(&input);
        assert!(
            started.elapsed() < BUDGET,
            "iteration {iteration} from {name} took {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn deeply_nested_markup_does_not_overflow_the_stack() {
    // The one shape most likely to find a recursive parser, run past anything
    // the mutator would reach by chance.
    for depth in [1_000usize, 10_000, 50_000] {
        let html = format!("{}text{}", "<div>".repeat(depth), "</div>".repeat(depth));
        let _ = rsst::article::layout(&rsst::article::parse(&html), 40, false);
        let _ = rsst::readable::extract(&html);
    }
}

#[test]
fn a_feed_cannot_make_the_renderer_allocate_without_bound() {
    // A width of one is the worst case for wrapping: every character its own
    // line. It must still finish.
    let html = "word ".repeat(20_000);
    let started = Instant::now();
    let rows = rsst::article::layout(&rsst::article::parse(&html), 1, false);
    assert!(started.elapsed() < BUDGET);
    assert!(!rows.is_empty());
}
