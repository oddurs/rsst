//! The one list of what every key does.
//!
//! Both the `--help` output and the in-app overlay are generated from this, so
//! a binding cannot be documented in one place and forgotten in the other. The
//! dispatcher in `main` still matches by hand; cairn 0027 (configurable
//! keybindings) is what makes it read this table too.

/// One row of the key reference.
pub struct Binding {
    pub keys: &'static str,
    pub action: &'static str,
}

/// A named group of bindings, in the order they should be shown.
pub struct Section {
    pub name: &'static str,
    pub bindings: &'static [Binding],
}

pub const SECTIONS: &[Section] = &[
    Section {
        name: "Moving",
        bindings: &[
            Binding {
                keys: "j / down",
                action: "next item, or scroll the detail pane",
            },
            Binding {
                keys: "k / up",
                action: "previous item, or scroll back",
            },
            Binding {
                keys: "g / G",
                action: "first / last",
            },
            Binding {
                keys: "Ctrl-d / Ctrl-u",
                action: "half a pane down / up",
            },
            Binding {
                keys: "Tab",
                action: "cycle feeds / entries / detail",
            },
            Binding {
                keys: "n / p",
                action: "next / previous unread, across feeds",
            },
        ],
    },
    Section {
        name: "Reading",
        bindings: &[
            Binding {
                keys: "m",
                action: "toggle read on the selected entry",
            },
            Binding {
                keys: "a / A",
                action: "mark this feed / every feed read (asks first)",
            },
            Binding {
                keys: "u",
                action: "show only unread entries",
            },
            Binding {
                keys: "s / S",
                action: "star the entry / show only starred",
            },
        ],
    },
    Section {
        name: "Finding",
        bindings: &[
            Binding {
                keys: "/",
                action: "search every feed",
            },
            Binding {
                keys: "n / N",
                action: "next / previous match, while searching",
            },
            Binding {
                keys: "Esc",
                action: "leave the search",
            },
        ],
    },
    Section {
        name: "Doing",
        bindings: &[
            Binding {
                keys: "o",
                action: "open the entry in your browser",
            },
            Binding {
                keys: "y",
                action: "copy its link to the clipboard",
            },
            Binding {
                keys: "r",
                action: "refresh all feeds",
            },
            Binding {
                keys: "?",
                action: "show this help",
            },
            Binding {
                keys: "q",
                action: "quit",
            },
        ],
    },
];

/// The width of the widest key column, so the two columns line up.
pub fn key_column_width() -> usize {
    SECTIONS
        .iter()
        .flat_map(|section| section.bindings)
        .map(|binding| binding.keys.len())
        .max()
        .unwrap_or(0)
}

/// The key reference as plain text, for `--help`.
pub fn as_text() -> String {
    let width = key_column_width();
    let mut out = String::new();
    for section in SECTIONS {
        out.push_str(&format!("\n{}:\n", section.name.to_uppercase()));
        for binding in section.bindings {
            out.push_str(&format!(
                "  {:width$}  {}\n",
                binding.keys,
                binding.action,
                width = width
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn all() -> Vec<&'static Binding> {
        SECTIONS.iter().flat_map(|s| s.bindings).collect()
    }

    #[test]
    fn every_section_has_bindings() {
        assert!(!SECTIONS.is_empty());
        assert!(SECTIONS.iter().all(|s| !s.bindings.is_empty()));
    }

    #[test]
    fn no_binding_is_listed_twice_in_the_same_section() {
        for section in SECTIONS {
            let mut seen = HashSet::new();
            for binding in section.bindings {
                assert!(
                    seen.insert(binding.keys),
                    "{} lists {} twice",
                    section.name,
                    binding.keys
                );
            }
        }
    }

    #[test]
    fn every_row_fits_an_eighty_column_terminal() {
        // Two leading spaces, the key column, a two-space gutter, the action,
        // and a border on each side.
        let width = key_column_width();
        for binding in all() {
            let rendered = 2 + width + 2 + binding.action.len() + 2;
            assert!(
                rendered <= 80,
                "{} + {} is {rendered} columns",
                binding.keys,
                binding.action
            );
        }
    }

    #[test]
    fn the_rendered_text_lists_every_binding() {
        let text = as_text();
        for binding in all() {
            assert!(text.contains(binding.keys), "missing {}", binding.keys);
            assert!(text.contains(binding.action), "missing {}", binding.action);
        }
    }

    #[test]
    fn quitting_and_help_are_documented() {
        let keys: Vec<_> = all().iter().map(|b| b.keys).collect();
        assert!(keys.contains(&"q"));
        assert!(keys.contains(&"?"));
    }
}
