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
    /// Write the man page to stdout.
    Man,
    /// Write a completion script for one shell to stdout.
    Completions(String),
    /// Render one frame as SVG and exit.
    Screenshot {
        size: String,
        config: Option<PathBuf>,
    },
}

/// The `--help` text, with the key reference generated from [`crate::keys`].
pub fn help(keymap: &crate::keys::Keymap) -> String {
    format!(
        "rsst {} — a terminal RSS/Atom feed reader

USAGE:
  rsst [OPTIONS]
  rsst import <FILE.opml> [OPTIONS]
  rsst export [OPTIONS] > subscriptions.opml

OPTIONS:
  -c, --config <PATH>  read this config instead of the default
      --man            write the man page to stdout
      --screenshot <WxH>
                       render one frame as SVG and exit
      --completions <SHELL>
                       write a completion script for bash, zsh or fish
  -h, --help           show this help
  -V, --version        show the version
{}
ENVIRONMENT:
  RSST_HOME            keep the config, database and read state in this
                       directory instead of the platform's own
  NO_COLOR             set to anything to disable colour

The config is written on first run; its path is reported if no feeds are set.",
        env!("CARGO_PKG_VERSION"),
        crate::keys::as_text(keymap)
    )
}

/// Parses arguments, excluding the program name.
pub fn parse<I, S>(args: I) -> Result<Action>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut config = None;
    let mut screenshot = None;
    let mut command = None;
    let mut import_path = None;
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "--man" => return Ok(Action::Man),
            "--screenshot" => {
                let size = args.next().context("--screenshot needs WIDTHxHEIGHT")?;
                screenshot = Some(size);
            }
            other if other.starts_with("--screenshot=") => {
                screenshot = Some(other.trim_start_matches("--screenshot=").to_string());
            }
            "--completions" => {
                let shell = args.next().context("--completions needs a shell")?;
                return Ok(Action::Completions(shell));
            }
            other if other.starts_with("--completions=") => {
                let shell = other.trim_start_matches("--completions=");
                if shell.is_empty() {
                    bail!("--completions needs a shell");
                }
                return Ok(Action::Completions(shell.to_string()));
            }
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

    if let Some(size) = screenshot {
        return Ok(Action::Screenshot { size, config });
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
    fn the_generators_are_reachable_from_the_command_line() {
        assert_eq!(parse_ok(&["--man"]), Action::Man);
        assert_eq!(
            parse_ok(&["--completions", "fish"]),
            Action::Completions("fish".into())
        );
        assert_eq!(
            parse_ok(&["--completions=zsh"]),
            Action::Completions("zsh".into())
        );
    }

    #[test]
    fn screenshot_takes_a_size_and_honours_config() {
        assert_eq!(
            parse_ok(&["--screenshot", "100x30"]),
            Action::Screenshot {
                size: "100x30".into(),
                config: None
            }
        );
        assert_eq!(
            parse_ok(&["--screenshot=80x24", "--config", "/tmp/a.toml"]),
            Action::Screenshot {
                size: "80x24".into(),
                config: Some(PathBuf::from("/tmp/a.toml"))
            }
        );
    }

    #[test]
    fn screenshot_without_a_size_is_an_error() {
        assert!(parse(["--screenshot"]).is_err());
    }

    #[test]
    fn completions_without_a_shell_is_an_error() {
        assert!(parse(["--completions"]).is_err());
        assert!(parse(["--completions="]).is_err());
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
        let help = help(&crate::keys::Keymap::default());
        assert!(help.starts_with("rsst "));
        assert!(help.contains("--config"));
        assert!(help.contains("quit"));
        assert!(help.contains("open the entry in your browser"));
    }
}
