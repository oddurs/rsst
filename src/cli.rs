use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// What the command line asked us to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the reader, optionally against a config other than the default.
    Run {
        config: Option<PathBuf>,
    },
    /// Merge an OPML subscription list into the config.
    Import {
        path: PathBuf,
        config: Option<PathBuf>,
    },
    /// Write the configured feeds out as OPML.
    Export {
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
  rsst import <FILE.opml> [OPTIONS]
  rsst export [OPTIONS] > subscriptions.opml

OPTIONS:
  -c, --config <PATH>  read this config instead of the default
  -h, --help           show this help
  -V, --version        show the version

KEYS:
  j / down             next item, or scroll the detail pane
  k / up               previous item, or scroll back
  Tab                  cycle feeds / entries / detail
  /                    search every feed; n and N step through matches
  u                    show only unread entries
  o                    open the selected entry in your browser
  y                    copy its link to the clipboard
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
    let mut command = None;
    let mut import_path = None;
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
            // Accepted because that is how the flag reads in every other
            // reader; OPML is the only export format there is.
            "--opml" => {}
            "import" | "export" if command.is_none() => command = Some(arg),
            other if !other.starts_with('-') && command.as_deref() == Some("import") => {
                if import_path.is_some() {
                    bail!("import takes one file");
                }
                import_path = Some(PathBuf::from(other));
            }
            other => bail!("unrecognised argument `{other}`\n\nTry `rsst --help`."),
        }
    }

    match command.as_deref() {
        Some("import") => Ok(Action::Import {
            path: import_path.context("import needs a path to an OPML file")?,
            config,
        }),
        Some("export") => Ok(Action::Export { config }),
        _ => Ok(Action::Run { config }),
    }
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
    fn import_takes_a_path() {
        assert_eq!(
            parse_ok(&["import", "subs.opml"]),
            Action::Import {
                path: PathBuf::from("subs.opml"),
                config: None
            }
        );
    }

    #[test]
    fn import_without_a_path_is_an_error() {
        assert!(parse(["import"]).is_err());
    }

    #[test]
    fn import_rejects_a_second_path() {
        assert!(parse(["import", "a.opml", "b.opml"]).is_err());
    }

    #[test]
    fn export_takes_no_path_and_tolerates_the_opml_flag() {
        assert_eq!(parse_ok(&["export"]), Action::Export { config: None });
        assert_eq!(
            parse_ok(&["export", "--opml"]),
            Action::Export { config: None }
        );
    }

    #[test]
    fn a_subcommand_still_honours_config() {
        assert_eq!(
            parse_ok(&["export", "--config", "/tmp/a.toml"]),
            Action::Export {
                config: Some(PathBuf::from("/tmp/a.toml"))
            }
        );
    }

    #[test]
    fn the_help_text_names_the_binary_and_the_keys() {
        assert!(HELP.starts_with("rsst "));
        assert!(HELP.contains("--config"));
        assert!(HELP.contains("q / Esc"));
        assert!(HELP.contains("open the selected entry"));
    }
}
