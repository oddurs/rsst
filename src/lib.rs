//! A terminal RSS/Atom feed reader.
//!
//! The binary in `main.rs` is the reader itself; everything it is built from
//! lives here so that benchmarks and integration tests can use the real code
//! rather than a copy of it.

pub mod app;
pub mod cache;
pub mod cli;
pub mod config;
pub mod feed;
pub mod generate;
pub mod keys;
pub mod launch;
pub mod limit;
pub mod opml;
pub mod screenshot;
pub mod state;
pub mod text;
pub mod theme;
pub mod ui;
