//! Env files: `.env`, `.env.example`, `prod.env`. Each `NAME=` line declares the variable `env NAME`, which is what
//! meets the code that reads it.

use gsttt_core::connector::{
    Connector, ConnectorError, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::coverage::language_of;
use gsttt_core::fact::{ContractFact, FileFacts, Import, Resolution, UnresolvedReason};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{any_claimed, line_number, provide, site};

const VIA: &str = "env";

/// The connector for env files.
#[derive(Clone, Copy, Debug)]
pub struct Env;

/// The connector for env files.
pub static ENV: Erased<Env> = Erased(Env);

/// The options of the `env` connector: it has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct EnvOptions {}

/// The project of the `env` connector.
#[derive(Clone, Copy, Debug)]
pub struct EnvProject;

/// `.env`, `.env.*` and `*.env`. A `.env.*` file in a programming language, `.env.d.ts` or `.env.schema.ts`, only has
/// a name that begins like an env file's, and reading its code for `NAME=` lines would invent variables.
fn claims(path: &RepoPath) -> bool {
    let name = path.file_name();
    let ends_with_env = name
        .len()
        .checked_sub(4)
        .and_then(|start| name.get(start..))
        .is_some_and(|tail| tail.eq_ignore_ascii_case(".env"));
    let is_source = path.extension().and_then(language_of).is_some();
    (name.starts_with(".env.") && !is_source) || ends_with_env
}

impl Connector for Env {
    type Options = EnvOptions;
    type Project = EnvProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "env",
            version: 1,
            summary: "Environment variables declared by .env files",
            extensions: &["env"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        any_claimed(repo, claims)
    }

    fn open(
        &self,
        repo: &RepoView,
        _: &EnvOptions,
        _: &[Pack],
    ) -> Result<Vec<EnvProject>, ConnectorError> {
        Ok(if any_claimed(repo, claims) {
            vec![EnvProject]
        } else {
            Vec::new()
        })
    }
}

impl Project for EnvProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        claims(path)
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        vec![provide(
            ContractKind::Env,
            Role::Declares,
            &["env files"],
            &[
                "environment variables set in compose files",
                "CI and deployment manifests",
            ],
        )]
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        Parsed::new(FileFacts {
            contracts: declarations(file.text),
            ..FileFacts::default()
        })
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::Unsupported(
            "env files have no imports".to_owned(),
        ))
    }
}

/// The variable declared at each `NAME=` line, with or without `export`. The lines of a quoted value that runs over
/// several lines declare nothing: a key or a certificate kept in a variable has lines that end in `=` like a name.
fn declarations(text: &str) -> Vec<ContractFact> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut facts = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let Some((name, value)) = assignment(line) else {
            continue;
        };
        facts.push(site(
            ContractKind::Env,
            name,
            Role::Declares,
            line_number(index),
            VIA,
        ));
        if let Some(quote) = unclosed_quote(value) {
            // The value ends at the line that closes the quote. A quote that nothing closes ends its value with its
            // own line, as dotenv reads it, so the lines after it still declare.
            let mut ahead = lines.clone();
            if ahead.any(|(_, later)| closes(later, quote)) {
                lines = ahead;
            }
        }
    }
    facts
}

/// The name and the value of a `NAME=value` line.
fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let declaration = line
        .strip_prefix("export")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map_or(line, str::trim_start);
    let (name, value) = declaration.split_once('=')?;
    let name = name.trim();
    let mut chars = name.chars();
    let starts_like_a_name = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    (starts_like_a_name && chars.all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then_some((name, value.trim_start()))
}

/// The quote that opens `value` and is not closed on the same line.
fn unclosed_quote(value: &str) -> Option<char> {
    let mut chars = value.chars();
    let quote = chars.next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    (!closes(chars.as_str(), quote)).then_some(quote)
}

/// Whether `text` holds a `quote` that a backslash does not escape.
fn closes(text: &str, quote: char) -> bool {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == quote {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use gsttt_core::fact::Line;
    use proptest::prelude::*;

    use super::*;

    /// Printable text without a quote or a backslash, so a value made of it opens no quote and closes none.
    const PLAIN: &str = "[ !#-&(-\\[\\]-_a-~]{0,12}";

    fn declared(text: &str) -> Vec<(Line, String)> {
        declarations(text)
            .into_iter()
            .map(|fact| (fact.line, fact.key.key().to_owned()))
            .collect()
    }

    proptest! {
        #[test]
        fn every_assignment_declares_its_name_at_its_line(
            lines in prop::collection::vec(
                prop_oneof![
                    ("[A-Za-z_][A-Za-z0-9_]{0,10}", PLAIN, any::<bool>()).prop_map(
                        |(name, value, export)| (
                            Some(name.clone()),
                            format!("{}{name}={value}", if export { "export " } else { "" }),
                        )
                    ),
                    Just((None, String::new())),
                    "[ -~]{0,20}".prop_map(|comment| (None, format!("# {comment}"))),
                ],
                0..12,
            )
        ) {
            let text = lines.iter().map(|(_, line)| line.as_str()).collect::<Vec<_>>().join("\n");
            let expected: Vec<(Line, String)> = lines
                .iter()
                .enumerate()
                .filter_map(|(index, (name, _))| {
                    name.as_ref().map(|name| (line_number(index), name.to_uppercase()))
                })
                .collect();
            prop_assert_eq!(declared(&text), expected);
        }

        #[test]
        fn lines_inside_a_quoted_value_declare_nothing_whatever_they_look_like(
            quote in prop::sample::select(vec!['"', '\'', '`']),
            start in PLAIN,
            body in prop::collection::vec(
                prop_oneof![("[A-Za-z_][A-Za-z0-9_]{0,10}", PLAIN).prop_map(|(name, value)| format!("{name}={value}")), PLAIN.prop_map(String::from)],
                0..8,
            ),
            end in PLAIN,
        ) {
            let mut lines = vec![format!("KEY={quote}{start}")];
            lines.extend(body.iter().cloned());
            lines.push(format!("{end}{quote}"));
            lines.push("AFTER=1".to_owned());
            let after = lines.len();
            prop_assert_eq!(
                declared(&lines.join("\n")),
                vec![(1, "KEY".to_owned()), (line_number(after - 1), "AFTER".to_owned())]
            );
        }

        #[test]
        fn arbitrary_text_never_panics(text in "\\PC*") {
            drop(declarations(&text));
        }
    }

    #[test]
    fn lines_that_assign_nothing_are_skipped() {
        let text = "# DATABASE_URL=x\n\nexport\nNOT A NAME=1\n1ST=2\nPORT = 3\n";
        assert_eq!(declared(text), vec![(6, "PORT".to_owned())]);
    }

    #[test]
    fn the_lines_of_a_quoted_value_declare_nothing() {
        let text = "KEY=\"-----BEGIN KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKc=\n-----END KEY-----\"\nPORT=3000\n";
        assert_eq!(
            declared(text),
            vec![(1, "KEY".to_owned()), (4, "PORT".to_owned())]
        );
    }

    #[test]
    fn every_kind_of_quote_holds_a_value_over_several_lines() {
        for quote in ['"', '\'', '`'] {
            let text = format!("A={quote}first\nB=1\nlast{quote}\nC=2");
            assert_eq!(
                declared(&text),
                vec![(1, "A".to_owned()), (4, "C".to_owned())],
                "{quote}"
            );
        }
    }

    #[test]
    fn an_escaped_quote_does_not_close_a_value() {
        let text = "A=\"one \\\" two\nB=1\nthree\"\nC=2";
        assert_eq!(
            declared(text),
            vec![(1, "A".to_owned()), (4, "C".to_owned())]
        );
    }

    #[test]
    fn a_quote_that_never_closes_ends_its_value_with_its_line() {
        let text = "A=\"one\nB=1\nC='two\nD=2";
        let names: Vec<_> = declared(text).into_iter().map(|(line, _)| line).collect();
        assert_eq!(names, vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_quote_closed_on_its_line_hides_nothing() {
        let text = "A=\"one\"\nB='two' # it's\nC=`three`\nD=1";
        let names: Vec<_> = declared(text).into_iter().map(|(line, _)| line).collect();
        assert_eq!(names, vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_source_file_named_like_an_env_file_is_not_one() {
        let claimed = |name: &str| claims(&RepoPath::try_from(name).unwrap());
        for name in [
            ".env",
            ".env.local",
            ".env.production.local",
            "config/.env.example",
            "prod.env",
        ] {
            assert!(claimed(name), "{name}");
        }
        for name in [
            ".env.d.ts",
            "src/.env.schema.ts",
            ".env.config.js",
            ".env.mjs",
            ".env.py",
            ".env.rs",
        ] {
            assert!(!claimed(name), "{name}");
        }
    }
}
