//! Env files: `.env`, `.env.example`, `prod.env`. Each `NAME=` line declares the variable `env NAME`, which is what
//! meets the code that reads it.

use gsttt_core::connector::{
    Connector, ConnectorError, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
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

/// `.env`, `.env.*` and `*.env`.
fn claims(path: &RepoPath) -> bool {
    let name = path.file_name();
    let ends_with_env = name
        .len()
        .checked_sub(4)
        .and_then(|start| name.get(start..))
        .is_some_and(|tail| tail.eq_ignore_ascii_case(".env"));
    name.starts_with(".env.") || ends_with_env
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

/// The variable declared at each `NAME=` line, with or without `export`.
fn declarations(text: &str) -> Vec<ContractFact> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let name = assigned_name(line)?;
            Some(site(
                ContractKind::Env,
                name,
                Role::Declares,
                line_number(index),
                VIA,
            ))
        })
        .collect()
}

fn assigned_name(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let assignment = line
        .strip_prefix("export")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map_or(line, str::trim_start);
    let (name, _) = assignment.split_once('=')?;
    let name = name.trim();
    let mut chars = name.chars();
    let starts_like_a_name = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    (starts_like_a_name && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(name)
}

#[cfg(test)]
mod tests {
    use gsttt_core::fact::Line;
    use proptest::prelude::*;

    use super::*;

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
                    ("[A-Za-z_][A-Za-z0-9_]{0,10}", "[ -~]{0,12}", any::<bool>()).prop_map(
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
        fn arbitrary_text_never_panics(text in "\\PC*") {
            drop(declarations(&text));
        }
    }

    #[test]
    fn lines_that_assign_nothing_are_skipped() {
        let text = "# DATABASE_URL=x\n\nexport\nNOT A NAME=1\n1ST=2\nPORT = 3\n";
        assert_eq!(declared(text), vec![(6, "PORT".to_owned())]);
    }
}
