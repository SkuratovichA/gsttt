//! `go.mod`: the module path and the modules it requires.

use std::collections::BTreeSet;

/// What a `go.mod` declares that the connector uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GoMod {
    /// The path other modules import this one by, empty when the file has no `module` line.
    pub(crate) module: String,
    /// The paths of the required modules, `// indirect` ones too: a package of a dependency reached through another is still a package of that dependency.
    pub(crate) requires: BTreeSet<String>,
}

/// Reads the text of a `go.mod`. A line it does not understand is left out, because the go tool, not this reader, is the judge of the file.
pub(crate) fn read_go_mod(text: &str) -> GoMod {
    let mut read = GoMod::default();
    // The directive a `(` opened, until its `)`.
    let mut block: Option<&str> = None;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if block.is_some() && line == ")" {
            block = None;
            continue;
        }
        let (directive, rest) = if let Some(directive) = block {
            (directive, line)
        } else {
            let (directive, rest) = line
                .split_once(char::is_whitespace)
                .map_or((line, ""), |(directive, rest)| (directive, rest.trim()));
            if rest == "(" {
                block = Some(directive);
                continue;
            }
            (directive, rest)
        };
        let Some(path) = rest.split_whitespace().next().map(unquoted) else {
            continue;
        };
        match directive {
            "module" if read.module.is_empty() => path.clone_into(&mut read.module),
            "require" => {
                read.requires.insert(path.to_owned());
            }
            _ => {}
        }
    }
    read
}

/// A path written in quotes, as `go.mod` allows, without them.
fn unquoted(path: &str) -> &str {
    path.trim_matches(['"', '`'])
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn the_module_line_names_the_module() {
        assert_eq!(
            read_go_mod("module example.com/app\n\ngo 1.22\n").module,
            "example.com/app"
        );
        assert_eq!(
            read_go_mod("module \"example.com/q\"").module,
            "example.com/q"
        );
    }

    #[test]
    fn requires_come_in_both_forms_and_with_indirect_ones() {
        let read = read_go_mod(
            "module m\nrequire github.com/a/b v1.0.0\nrequire (\n\tgithub.com/c/d v1.2.3\n\tgithub.com/e/f v0.1.0 // indirect\n)\nreplace github.com/a/b => ../b\n",
        );
        assert_eq!(
            read.requires.into_iter().collect::<Vec<_>>(),
            ["github.com/a/b", "github.com/c/d", "github.com/e/f"]
        );
    }

    #[test]
    fn a_file_without_a_module_line_has_an_empty_module() {
        assert_eq!(read_go_mod("go 1.22\n").module, "");
    }

    proptest! {
        #[test]
        fn no_text_makes_the_reader_fail(text in ".*") {
            let read = read_go_mod(&text);
            for path in &read.requires {
                prop_assert!(!path.contains(char::is_whitespace));
            }
        }
    }
}
