//! Reading one source file: a syntax tree even when the file has errors, and the references that live in comments.

use std::borrow::Cow;

use gsttt_core::connector::{Parsed, SourceFile};
use gsttt_core::fact::{
    ErrorCount, FileFacts, FileTags, Import, ImportKind, Line, ParseFailure, ParseStatus,
};
use gsttt_core::rules::Pack;
use gsttt_core::text::LineIndex;
use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::{LabeledSpan, SourceType};

use crate::analyze::{self, Analysis};
use crate::extra::TsExtra;

/// The extensions of the files this connector reads as code.
pub(crate) const CODE_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// How many times a file is parsed again with the line of its first error blanked.
const MAX_ATTEMPTS: usize = 24;

/// Whether the name or path ends in an extension this connector reads.
pub(crate) fn is_code(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| CODE_EXTENSIONS.contains(&extension))
}

/// Parses `file`. A file whose parse cannot go on has the line of its first error blanked and is parsed again, so the imports above and below one bad line survive; only a file that still fails after that is reported as failed.
pub(crate) fn parse_code(file: &SourceFile<'_>, packs: &[Pack]) -> Parsed<TsExtra> {
    let source_type =
        SourceType::from_path(file.path.as_str()).unwrap_or_else(|_| SourceType::ts());
    let index = LineIndex::new(file.text);
    let mut text = Cow::Borrowed(file.text);
    let mut error_lines = ErrorCount::default();
    let mut failure: Option<(Line, String)> = None;
    for _ in 0..MAX_ATTEMPTS {
        let allocator = Allocator::default();
        let options = ParseOptions {
            allow_return_outside_function: true,
            preserve_parens: false,
            ..ParseOptions::default()
        };
        let parsed = Parser::new(&allocator, &text, source_type)
            .with_options(options)
            .parse();
        let errors: Vec<(Line, String)> = parsed
            .diagnostics
            .errors()
            .map(|error| {
                let offset = error.labels.first().map_or(0, LabeledSpan::offset);
                (index.line(offset), error.message.to_string())
            })
            .collect();
        if !parsed.fatal_error {
            let analysis = analyze::analyze(&parsed.program, &text, &index, packs);
            // The lines blanked by the earlier attempts are errors of the file, and so are the ones this parse still reports.
            for (line, _) in &errors {
                error_lines.note(*line);
            }
            return assemble(&text, analysis, error_lines.status());
        }
        let line = errors.first().map_or(1, |(line, _)| *line);
        if failure.is_none() {
            let message = errors.first().map_or_else(
                || "the file could not be parsed".to_owned(),
                |(_, message)| message.clone(),
            );
            failure = Some((line, message));
        }
        let Some(next) = blank_line(&text, line) else {
            break;
        };
        error_lines.note(line);
        text = Cow::Owned(next);
    }
    let (line, message) = failure.unwrap_or_else(|| (1, "the file could not be parsed".to_owned()));
    Parsed::new(FileFacts::failed(ParseFailure::Syntax { line, message }))
}

fn assemble(text: &str, analysis: Analysis, status: ParseStatus) -> Parsed<TsExtra> {
    let mut imports: Vec<Import> = references(text)
        .into_iter()
        .map(|(spec, line)| Import::new(spec, ImportKind::Reference, line))
        .collect();
    imports.extend(analysis.imports);
    // A file read past errors may have lost an export with the line that held it.
    let exports = analysis.exports.filter(|_| status == ParseStatus::Ok);
    Parsed {
        facts: FileFacts {
            status,
            tags: FileTags::NONE,
            imports,
            contracts: analysis.contracts,
            embedded: analysis.embedded,
            unresolved: analysis.unresolved,
            exports,
            symbols: None,
        },
        extra: analysis.extra,
    }
}

/// The text with the non-blank characters of `line` (1-based) replaced by spaces of the same byte length, so every offset and line number stays the same. `None` when the line has nothing left to blank.
fn blank_line(text: &str, line: Line) -> Option<String> {
    let wanted = usize::try_from(line).ok()?.checked_sub(1)?;
    let mut changed = false;
    let mut blanked = String::with_capacity(text.len());
    for (number, row) in text.split_inclusive('\n').enumerate() {
        if number != wanted {
            blanked.push_str(row);
            continue;
        }
        for character in row.chars() {
            if matches!(character, '\n' | '\r') {
                blanked.push(character);
            } else {
                changed |= !character.is_whitespace();
                blanked.extend(std::iter::repeat_n(' ', character.len_utf8()));
            }
        }
    }
    changed.then_some(blanked)
}

/// The `/// <reference path="…" />` lines at the top of a file, with their lines.
fn references(text: &str) -> Vec<(String, Line)> {
    let mut found = Vec::new();
    for (number, row) in text.lines().enumerate() {
        let trimmed = row.trim_start();
        if let Some(path) = reference_path(trimmed) {
            let line = Line::try_from(number.saturating_add(1)).unwrap_or(Line::MAX);
            found.push((path.to_owned(), line));
        } else if !(trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*'))
        {
            break;
        }
    }
    found
}

/// The `path` of a triple-slash reference directive written on `row`.
fn reference_path(row: &str) -> Option<&str> {
    let rest = row
        .strip_prefix("///")?
        .trim_start()
        .strip_prefix("<reference")?;
    let value = rest
        .split_whitespace()
        .find_map(|token| token.strip_prefix("path="))?;
    let value = value.trim_end_matches("/>").trim_end_matches('>');
    let quote = value
        .chars()
        .next()
        .filter(|character| matches!(character, '"' | '\''))?;
    value.get(1..)?.split(quote).next()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_reference_directive_names_its_path() {
        assert_eq!(
            reference_path(r#"/// <reference path="./globals.d.ts" />"#),
            Some("./globals.d.ts")
        );
        assert_eq!(reference_path("/// <reference path='a.ts'/>"), Some("a.ts"));
        assert_eq!(reference_path(r#"/// <reference types="node" />"#), None);
        assert_eq!(reference_path("// <reference path=\"a.ts\" />"), None);
    }

    #[test]
    fn references_stop_at_the_first_statement() {
        let text = "/// <reference path=\"a.d.ts\" />\n\nimport x from 'x';\n/// <reference path=\"b.d.ts\" />\n";
        assert_eq!(references(text), vec![("a.d.ts".to_owned(), 1)]);
    }

    #[test]
    fn blanking_keeps_every_offset() {
        // `cé d` is five bytes long, so five spaces take its place.
        assert_eq!(
            blank_line("a b\ncé d\ne\n", 2).as_deref(),
            Some("a b\n     \ne\n")
        );
        assert_eq!(blank_line("a\n   \nb", 2), None);
    }

    proptest! {
        #[test]
        fn blanking_never_moves_a_line(text in "[a-z é\n]{0,40}", line in 0_u32..8) {
            if let Some(blanked) = blank_line(&text, line) {
                prop_assert_eq!(blanked.len(), text.len());
                prop_assert_eq!(blanked.matches('\n').count(), text.matches('\n').count());
            }
        }

        #[test]
        fn a_reference_scan_never_panics(text in "[ -~\n]{0,80}") {
            drop(references(&text));
            let _ = reference_path(&text);
        }
    }
}
