//! Generating the man page and shell completions from the CLI definition.
//!
//! Written out by `rsst --man` and `rsst --completions <shell>`, which the
//! release workflow calls. Generated rather than hand-maintained so a new flag
//! or binding cannot be documented in one place and missed in the others.

use anyhow::{Result, bail};

use crate::keys::Keymap;

/// Every flag rsst accepts: `(short, long, argument, description)`.
pub const FLAGS: &[(Option<&str>, &str, Option<&str>, &str)] = &[
    (
        Some("-c"),
        "--config",
        Some("PATH"),
        "read this config instead of the default",
    ),
    (None, "--man", None, "write the man page to stdout"),
    (
        None,
        "--completions",
        Some("SHELL"),
        "write a completion script for bash, zsh or fish",
    ),
    (Some("-h"), "--help", None, "show help"),
    (Some("-V"), "--version", None, "show the version"),
];

/// The subcommands, as `(name, argument, description)`.
pub const SUBCOMMANDS: &[(&str, Option<&str>, &str)] = &[
    (
        "import",
        Some("FILE"),
        "merge an OPML subscription list into the config",
    ),
    ("export", None, "write the configured feeds out as OPML"),
];

/// The man page, in roff.
pub fn man(keymap: &Keymap) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut out = format!(
        ".TH RSST 1 \"\" \"rsst {version}\" \"User Commands\"\n\
         .SH NAME\n\
         rsst \\- a terminal RSS/Atom feed reader\n\
         .SH SYNOPSIS\n\
         .B rsst\n\
         [\\fIOPTIONS\\fR]\n\
         .br\n\
         .B rsst import\n\
         \\fIFILE.opml\\fR\n\
         .br\n\
         .B rsst export\n\
         .SH DESCRIPTION\n\
         Three panes: feeds on the left, that feed's entries top right, the\n\
         selected entry's text below. Feeds are fetched concurrently and cached,\n\
         so launching is instant and a reader with no network still shows the\n\
         last entries it fetched.\n\
         .SH OPTIONS\n"
    );

    for (short, long, argument, description) in FLAGS {
        let names = match short {
            Some(short) => format!("{short}, {long}"),
            None => (*long).to_string(),
        };
        let names = match argument {
            Some(argument) => format!("{names} \\fI{argument}\\fR"),
            None => names,
        };
        out.push_str(&format!(".TP\n.B {names}\n{description}\n"));
    }

    out.push_str(".SH COMMANDS\n");
    for (name, argument, description) in SUBCOMMANDS {
        let names = match argument {
            Some(argument) => format!("{name} \\fI{argument}\\fR"),
            None => (*name).to_string(),
        };
        out.push_str(&format!(".TP\n.B {names}\n{description}\n"));
    }

    out.push_str(".SH KEYS\n");
    for (section, rows) in keymap.sections() {
        out.push_str(&format!(".SS {section}\n"));
        for (keys, description) in rows {
            out.push_str(&format!(".TP\n.B {keys}\n{description}\n"));
        }
    }

    out.push_str(
        ".SH FILES\n\
         .TP\n\
         .B $XDG_CONFIG_HOME/rsst/config.toml\n\
         Feeds, colours and key bindings.\n\
         .TP\n\
         .B $XDG_DATA_HOME/rsst/read.toml\n\
         Which entries have been read and starred.\n\
         .TP\n\
         .B $XDG_CACHE_HOME/rsst/feeds.toml\n\
         Last known feed contents.\n\
         .SH ENVIRONMENT\n\
         .TP\n\
         .B RSST_HOME\n\
         Keep the config, the database and the read state in this directory\n\
         rather than the platform's own. Everything above moves together, so a\n\
         second rsst run this way cannot touch the first one's data.\n\
         .TP\n\
         .B NO_COLOR\n\
         Set to anything to disable colour, whatever the config says.\n",
    );
    out
}

/// A completion script for one shell.
pub fn completions(shell: &str) -> Result<String> {
    let longs: Vec<&str> = FLAGS.iter().map(|(_, long, ..)| *long).collect();
    let shorts: Vec<&str> = FLAGS.iter().filter_map(|(short, ..)| *short).collect();
    let commands: Vec<&str> = SUBCOMMANDS.iter().map(|(name, ..)| *name).collect();
    let all = [longs.clone(), shorts.clone()].concat().join(" ");

    Ok(match shell {
        "bash" => format!(
            "# rsst completions for bash\n\
             _rsst() {{\n\
             \x20 local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n\
             \x20 local prev=\"${{COMP_WORDS[COMP_CWORD-1]}}\"\n\
             \x20 case \"$prev\" in\n\
             \x20   -c|--config) COMPREPLY=( $(compgen -f -- \"$cur\") ); return ;;\n\
             \x20   --completions) COMPREPLY=( $(compgen -W \"bash zsh fish\" -- \"$cur\") ); return ;;\n\
             \x20   import) COMPREPLY=( $(compgen -f -X '!*.opml' -- \"$cur\") ); return ;;\n\
             \x20 esac\n\
             \x20 COMPREPLY=( $(compgen -W \"{all} {commands}\" -- \"$cur\") )\n\
             }}\n\
             complete -F _rsst rsst\n",
            all = all,
            commands = commands.join(" ")
        ),
        "zsh" => {
            let mut out = String::from("#compdef rsst\n\n_rsst() {\n  _arguments -s \\\n");
            for (short, long, argument, description) in FLAGS {
                let spec = match argument {
                    Some("PATH") => ":config file:_files",
                    Some("SHELL") => ":shell:(bash zsh fish)",
                    Some(_) => ":value:",
                    None => "",
                };
                if let Some(short) = short {
                    out.push_str(&format!("    '{short}[{description}]{spec}' \\\n"));
                }
                out.push_str(&format!("    '{long}[{description}]{spec}' \\\n"));
            }
            out.push_str("    '1:command:((");
            for (name, _, description) in SUBCOMMANDS {
                out.push_str(&format!("{name}\\:'{description}' "));
            }
            // No trailing call: a #compdef file is autoloaded by the
            // completion system, which invokes the function itself.
            out.push_str("))' \\\n    '*:file:_files'\n}\n");
            out
        }
        "fish" => {
            let mut out = String::from("# rsst completions for fish\n");
            for (short, long, argument, description) in FLAGS {
                let mut line = format!("complete -c rsst -l {}", long.trim_start_matches("--"));
                if let Some(short) = short {
                    line.push_str(&format!(" -s {}", short.trim_start_matches('-')));
                }
                match argument {
                    Some("PATH") => line.push_str(" -r -F"),
                    Some("SHELL") => line.push_str(" -x -a 'bash zsh fish'"),
                    Some(_) => line.push_str(" -x"),
                    None => {}
                }
                line.push_str(&format!(" -d '{description}'\n"));
                out.push_str(&line);
            }
            for (name, _, description) in SUBCOMMANDS {
                out.push_str(&format!(
                    "complete -c rsst -n __fish_use_subcommand -a {name} -d '{description}'\n"
                ));
            }
            out
        }
        other => bail!("no completions for `{other}`. Try bash, zsh or fish."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_man_page_documents_every_flag_and_command() {
        let page = man(&Keymap::default());
        for (_, long, _, description) in FLAGS {
            assert!(page.contains(long), "man page is missing {long}");
            assert!(
                page.contains(description),
                "man page is missing {description}"
            );
        }
        for (name, _, description) in SUBCOMMANDS {
            assert!(page.contains(name));
            assert!(page.contains(description));
        }
    }

    #[test]
    fn the_man_page_documents_every_key() {
        let keymap = Keymap::default();
        let page = man(&keymap);
        for (_, rows) in keymap.sections() {
            for (keys, description) in rows {
                assert!(page.contains(&keys), "man page is missing key {keys}");
                assert!(page.contains(description));
            }
        }
    }

    #[test]
    fn the_man_page_starts_with_a_roff_header() {
        assert!(man(&Keymap::default()).starts_with(".TH RSST 1"));
    }

    #[test]
    fn every_supported_shell_gets_a_script_naming_the_binary() {
        for shell in ["bash", "zsh", "fish"] {
            let script = completions(shell).expect("supported");
            assert!(
                script.contains("rsst"),
                "{shell} script does not mention rsst"
            );
            assert!(!script.trim().is_empty());
        }
    }

    #[test]
    fn completions_list_every_long_flag() {
        for shell in ["bash", "zsh", "fish"] {
            let script = completions(shell).expect("supported");
            for (_, long, ..) in FLAGS {
                let needle = long.trim_start_matches("--");
                assert!(script.contains(needle), "{shell} is missing {long}");
            }
        }
    }

    #[test]
    fn completions_offer_both_subcommands() {
        for shell in ["bash", "zsh", "fish"] {
            let script = completions(shell).expect("supported");
            assert!(script.contains("import"));
            assert!(script.contains("export"));
        }
    }

    #[test]
    fn an_unsupported_shell_is_reported() {
        let err = completions("tcsh").expect_err("rejected");
        assert!(err.to_string().contains("bash, zsh or fish"));
    }
}
