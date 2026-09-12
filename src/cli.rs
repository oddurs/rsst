use std::path::PathBuf;

use anyhow::{Result, bail};

/// What the command line asked us to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the reader, optionally against a config other than the default.
    Run {
        config: Option<PathBuf>,
    },
    Help,
    Version,
}

pub const HELP: &str = concat!(
    "rsst ",
    env!("CARGO_PKG_VERSION"),
    " — a terminal RSS/Atom feed reader

USAGE:
  rsst [OPTIONS]

OPTIONS:
  -c, --config <PATH>  read this config instead of the default
  -h, --help           show this help
  -V, --version        show the version

KEYS:
  j / down             next item
  k / up               previous item
  Tab                  switch panes
  r                    refresh all feeds
  q / Esc              quit

The config is written on first run; its path is reported if no feeds are set."
);

/// Parses arguments, excluding the program name.
pub fn parse<I, S>(args: I) -> Result<Action>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut config = None;
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "-c" | "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--config needs a path"))?;
                config = Some(PathBuf::from(path));
            }
            // `--config=PATH` is the other spelling people reach for.
            other if other.starts_with("--config=") => {
                let path = other.trim_start_matches("--config=");
                if path.is_empty() {
                    bail!("--config needs a path");
                }
                config = Some(PathBuf::from(path));
            }
            other => bail!("unrecognised argument `{other}`\n\nTry `rsst --help`."),
        }
    }

    Ok(Action::Run { config })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(args: &[&str]) -> Action {
        parse(args.iter().copied()).expect("should parse")
    }

    #[test]
    fn no_arguments_runs_with_the_default_config() {
        assert_eq!(parse_ok(&[]), Action::Run { config: None });
    }

    #[test]
    fn help_and_version_are_recognised_in_both_spellings() {
        assert_eq!(parse_ok(&["-h"]), Action::Help);
        assert_eq!(parse_ok(&["--help"]), Action::Help);
        assert_eq!(parse_ok(&["-V"]), Action::Version);
        assert_eq!(parse_ok(&["--version"]), Action::Version);
    }

    #[test]
    fn config_accepts_a_separate_or_joined_value() {
        let expected = Action::Run {
            config: Some(PathBuf::from("/tmp/a.toml")),
        };
        assert_eq!(parse_ok(&["--config", "/tmp/a.toml"]), expected);
        assert_eq!(parse_ok(&["-c", "/tmp/a.toml"]), expected);
        assert_eq!(parse_ok(&["--config=/tmp/a.toml"]), expected);
    }

    #[test]
    fn an_unknown_flag_is_an_error() {
        let err = parse(["--nope"]).expect_err("should reject");
        assert!(err.to_string().contains("--nope"), "names the bad argument");
    }

    #[test]
    fn config_without_a_value_is_an_error() {
        assert!(parse(["--config"]).is_err());
        assert!(parse(["--config="]).is_err());
    }

    #[test]
    fn help_wins_over_a_later_bad_argument() {
        // Someone reaching for --help should get help, not a parse error.
        assert_eq!(parse_ok(&["--help", "--nope"]), Action::Help);
    }

    #[test]
    fn the_help_text_names_the_binary_and_the_keys() {
        assert!(HELP.starts_with("rsst "));
        assert!(HELP.contains("--config"));
        assert!(HELP.contains("q / Esc"));
    }
}
