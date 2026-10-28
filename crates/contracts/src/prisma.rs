//! Prisma schemas. Each `model` declares `table <model>` and, with `@@map("<name>")`, also `table <name>`; a
//! datasource property `url = env("NAME")` calls `env NAME`.

use gsttt_core::connector::{
    Connector, ConnectorError, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    ContractFact, ErrorCount, FileFacts, Import, Line, ParseStatus, Resolution, UnresolvedReason,
};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{any_claimed, line_number, provide, site};

const VIA: &str = "prisma";

/// The connector for Prisma schemas.
#[derive(Clone, Copy, Debug)]
pub struct Prisma;

/// The connector for Prisma schemas.
pub static PRISMA: Erased<Prisma> = Erased(Prisma);

/// The options of the `prisma` connector: it has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PrismaOptions {}

/// The project of the `prisma` connector.
#[derive(Clone, Copy, Debug)]
pub struct PrismaProject;

fn claims(path: &RepoPath) -> bool {
    path.extension() == Some("prisma")
}

impl Connector for Prisma {
    type Options = PrismaOptions;
    type Project = PrismaProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "prisma",
            version: 1,
            summary: "Tables declared by Prisma models and the environment variables of the datasource",
            extensions: &["prisma"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        any_claimed(repo, claims)
    }

    fn open(
        &self,
        repo: &RepoView,
        _: &PrismaOptions,
        _: &[Pack],
    ) -> Result<Vec<PrismaProject>, ConnectorError> {
        Ok(if any_claimed(repo, claims) {
            vec![PrismaProject]
        } else {
            Vec::new()
        })
    }
}

impl Project for PrismaProject {
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
        vec![
            provide(
                ContractKind::Table,
                Role::Declares,
                &["Prisma models"],
                &["schemas generated at build time"],
            ),
            provide(
                ContractKind::Env,
                Role::Calls,
                &["Prisma datasource properties"],
                &["environment variables read outside the datasource block"],
            ),
        ]
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        let (contracts, status) = read(file.text);
        Parsed::new(FileFacts {
            status,
            contracts,
            ..FileFacts::default()
        })
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::Unsupported(
            "Prisma schemas have no imports".to_owned(),
        ))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Model,
    Datasource,
    Other,
}

/// Reads a schema line by line: Prisma's blocks never nest and never share a line with another block's body, so a
/// scanner finds everything the contracts need without a grammar. A block that never closes and a closing brace
/// that closes nothing are the recovered errors.
fn read(text: &str) -> (Vec<ContractFact>, ParseStatus) {
    let mut facts = Vec::new();
    let mut errors = ErrorCount::default();
    let mut open: Option<(Block, Line)> = None;
    for (index, raw) in text.lines().enumerate() {
        let number = line_number(index);
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let (kind, body) = if let Some((kind, _)) = open {
            (kind, line)
        } else {
            if line.starts_with('}') {
                errors.note(number);
                continue;
            }
            let Some((keyword, name, rest)) = block_head(line) else {
                continue;
            };
            let kind = match keyword {
                "model" => Block::Model,
                "datasource" => Block::Datasource,
                _ => Block::Other,
            };
            if kind == Block::Model {
                facts.push(site(ContractKind::Table, name, Role::Declares, number, VIA));
            }
            open = Some((kind, number));
            (kind, rest)
        };
        // A brace inside a string, as in `@default("{}")`, closes nothing.
        let (body, closes) = match find_outside_strings(body, |c, _| c == '}') {
            Some(close) => (body.get(..close).unwrap_or(body), true),
            None => (body, false),
        };
        scan_body(kind, body, number, &mut facts);
        if closes {
            open = None;
        }
    }
    if let Some((_, opened)) = open {
        errors.note(opened);
    }
    (facts, errors.status())
}

/// `model BookingItem {` as the keyword, the name and what follows the brace, for the blocks Prisma has.
fn block_head(line: &str) -> Option<(&str, &str, &str)> {
    let (head, rest) = line.split_once('{')?;
    let mut words = head.split_whitespace();
    let keyword = words.next()?;
    let name = words.next()?;
    matches!(
        keyword,
        "model" | "view" | "type" | "enum" | "datasource" | "generator"
    )
    .then_some((keyword, name, rest))
}

fn scan_body(kind: Block, body: &str, line: Line, facts: &mut Vec<ContractFact>) {
    match kind {
        Block::Model => {
            if let Some(name) = quoted_argument(body, "@@map(") {
                facts.push(site(ContractKind::Table, name, Role::Declares, line, VIA));
            }
        }
        Block::Datasource => {
            if let Some(name) = quoted_argument(body, "env(") {
                facts.push(site(ContractKind::Env, name, Role::Calls, line, VIA));
            }
        }
        Block::Other => {}
    }
}

/// The first string literal after `marker`: `"booking_items"` in `@@map("booking_items")` and in
/// `@@map(name: "booking_items")`.
fn quoted_argument<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let after = text.get(text.find(marker)?.checked_add(marker.len())?..)?;
    let literal = after.get(after.find('"')?.checked_add(1)?..)?;
    let name = literal.get(..literal.find('"')?)?;
    (!name.is_empty()).then_some(name)
}

/// The line without its `//` comment; a `//` inside a string literal is part of the string.
fn strip_comment(line: &str) -> &str {
    find_outside_strings(line, |c, next| c == '/' && next == Some('/'))
        .and_then(|comment| line.get(..comment))
        .unwrap_or(line)
}

/// The byte offset of the first character that `wanted` accepts and that stands outside a string literal; `wanted`
/// gets the character and the one after it. A string ends at its next quote unless a backslash escapes it, and a
/// string never continues on the next line, so the scan starts outside one on every line.
fn find_outside_strings(line: &str, wanted: impl Fn(char, Option<char>) -> bool) -> Option<usize> {
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = line.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '"' {
            in_string = true;
        } else if wanted(c, chars.peek().map(|&(_, next)| next)) {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use proptest::prelude::*;

    use super::*;

    fn tables(text: &str) -> Vec<(Line, String)> {
        read(text)
            .0
            .into_iter()
            .filter(|fact| *fact.key.kind() == ContractKind::Table)
            .map(|fact| (fact.line, fact.key.key().to_owned()))
            .collect()
    }

    proptest! {
        #[test]
        fn every_model_declares_its_name_and_its_mapped_table(
            models in prop::collection::vec(
                ("[A-Z][A-Za-z0-9]{0,8}", prop::option::of("[a-z][a-z0-9_]{0,8}")),
                0..8,
            )
        ) {
            let mut text = String::new();
            let mut expected = Vec::new();
            for (name, mapped) in &models {
                let start = line_number(text.lines().count());
                writeln!(text, "model {name} {{\n  id Int @id").unwrap();
                expected.push((start, name.to_lowercase()));
                if let Some(mapped) = mapped {
                    writeln!(text, "  @@map(\"{mapped}\")").unwrap();
                    expected.push((start + 2, mapped.clone()));
                }
                text.push_str("}\n");
            }
            prop_assert_eq!(tables(&text), expected);
            prop_assert_eq!(read(&text).1, ParseStatus::Ok);
        }

        #[test]
        fn arbitrary_text_never_panics_and_keeps_lines_in_range(text in "\\PC*") {
            let lines = u32::try_from(text.lines().count().max(1)).unwrap();
            for fact in read(&text).0 {
                prop_assert!(fact.line >= 1 && fact.line <= lines);
            }
        }
    }

    #[test]
    fn a_brace_inside_a_string_does_not_close_the_block() {
        let text = "model A {\n  meta Json @default(\"{}\")\n  @@map(\"x\")\n}\n";
        assert_eq!(tables(text), vec![(1, "a".to_owned()), (3, "x".to_owned())]);
        assert_eq!(read(text).1, ParseStatus::Ok);
        // The same on the line that opens the block, and with an escaped quote before the brace.
        let text = "model B { n String @default(\"\\\"}\") @@map(\"y\") }\n";
        assert_eq!(tables(text), vec![(1, "b".to_owned()), (1, "y".to_owned())]);
        assert_eq!(read(text).1, ParseStatus::Ok);
    }

    #[test]
    fn comments_and_unclosed_blocks_are_read_with_care() {
        let text = "// model Ghost {\nmodel A { // still A\n  url String @default(\"//x\")\n";
        assert_eq!(tables(text), vec![(2, "a".to_owned())]);
        assert_eq!(
            read(text).1,
            ParseStatus::Recovered {
                errors: 1,
                first_error_line: 2
            }
        );
        assert_eq!(
            read("}\nmodel B {}\n").1,
            ParseStatus::Recovered {
                errors: 1,
                first_error_line: 1
            }
        );
    }
}
