//! Reading one source file: a syntax tree even when the file has errors, and the references that live in comments.

use std::borrow::Cow;

use gsttt_core::connector::{Parsed, SourceFile};
use gsttt_core::fact::{
    ErrorCount, FileFacts, FileTags, Import, ImportKind, Line, ParseFailure, ParseStatus,
};
use gsttt_core::rules::Pack;
use gsttt_core::text::LineIndex;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Statement, TemplateElement};
use oxc_ast_visit::{Visit, walk};
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

/// The source type of `file`, from its extension, TypeScript when the extension says none. The facts and the blanked code must parse alike, so both take it from here.
fn source_type_of(file: &SourceFile<'_>) -> SourceType {
    SourceType::from_path(file.path.as_str()).unwrap_or_else(|_| SourceType::ts())
}

/// The options every parse of a file uses, for the same reason as [`source_type_of`].
fn parse_options() -> ParseOptions {
    ParseOptions {
        allow_return_outside_function: true,
        preserve_parens: false,
        ..ParseOptions::default()
    }
}

/// Parses `file`. A file whose parse cannot go on has the line of its first error blanked and is parsed again, so the imports above and below one bad line survive; only a file that still fails after that is reported as failed.
pub(crate) fn parse_code(file: &SourceFile<'_>, packs: &[Pack], tags: FileTags) -> Parsed<TsExtra> {
    let source_type = source_type_of(file);
    let index = LineIndex::new(file.text);
    let mut text = Cow::Borrowed(file.text);
    let mut error_lines = ErrorCount::default();
    let mut failure: Option<(Line, String)> = None;
    for _ in 0..MAX_ATTEMPTS {
        let allocator = Allocator::default();
        let options = parse_options();
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
            let analysis = analyze::analyze(&parsed.program, &text, &index, packs, tags);
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

/// The byte ranges of the text of every template literal.
#[derive(Default)]
struct Templates(Vec<(usize, usize)>);

impl<'a> Visit<'a> for Templates {
    fn visit_template_element(&mut self, element: &TemplateElement<'a>) {
        if let (Ok(start), Ok(end)) = (
            usize::try_from(element.span.start),
            usize::try_from(element.span.end),
        ) {
            self.0.push((start, end));
        }
        walk::walk_template_element(self, element);
    }
}

/// The text of `file` with every comment and every import statement blanked, each byte but a line break becoming a space, so that the lines are the lines of the file. Inside a template literal whitespace is part of the value, so a space becomes `\x01`, a tab `\x02` and a line break is preceded by `\x03`: indenting or a blank line there still differs once a line is trimmed. Two versions of a file are compared on it. `None` when the parser cannot go on past the errors of the text.
pub(crate) fn blanked_code(file: &SourceFile<'_>) -> Option<String> {
    let source_type = source_type_of(file);
    let allocator = Allocator::default();
    let options = parse_options();
    let parsed = Parser::new(&allocator, file.text, source_type)
        .with_options(options)
        .parse();
    if parsed.fatal_error {
        return None;
    }
    let imports = parsed
        .program
        .body
        .iter()
        .filter_map(|statement| match statement {
            Statement::ImportDeclaration(import) => Some(import.span),
            _ => None,
        });
    let spans = parsed
        .program
        .comments
        .iter()
        .map(|comment| comment.span)
        .chain(imports);
    let mut bytes = file.text.as_bytes().to_vec();
    for span in spans {
        let range = usize::try_from(span.start).ok()?..usize::try_from(span.end).ok()?;
        for byte in bytes.get_mut(range)? {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
    }
    let mut templates = Templates::default();
    templates.visit_program(&parsed.program);
    templates.0.sort_unstable();
    let mut ranges = templates.0.into_iter().peekable();
    let mut marked = Vec::with_capacity(bytes.len());
    for (index, &byte) in bytes.iter().enumerate() {
        while ranges.next_if(|&(_, end)| end <= index).is_some() {}
        let inside = ranges
            .peek()
            .is_some_and(|&(start, end)| start <= index && index < end);
        match byte {
            b' ' if inside => marked.push(1),
            b'\t' if inside => marked.push(2),
            b'\n' if inside => marked.extend([3, b'\n']),
            _ => marked.push(byte),
        }
    }
    String::from_utf8(marked).ok()
}

fn assemble(text: &str, analysis: Analysis, status: ParseStatus) -> Parsed<TsExtra> {
    let mut imports: Vec<Import> = references(text)
        .into_iter()
        .map(|(spec, line)| Import::new(spec, ImportKind::Reference, line))
        .collect();
    imports.extend(analysis.imports);
    // A file read past errors may have lost an export or a declaration with the line that held it.
    let exports = analysis.exports.filter(|_| status == ParseStatus::Ok);
    let symbols = analysis.symbols.filter(|_| status == ParseStatus::Ok);
    if symbols.is_none() {
        // The local name of an import is recorded with the declarations that use it, and a file read past errors has none. A re-export keeps the names it exports.
        for import in imports
            .iter_mut()
            .filter(|import| import.kind == ImportKind::Static)
        {
            import.exported.clear();
        }
    }
    Parsed {
        facts: FileFacts {
            status,
            tags: FileTags::NONE,
            imports,
            contracts: analysis.contracts,
            embedded: analysis.embedded,
            unresolved: analysis.unresolved,
            exports,
            symbols,
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

    fn blanked(path: &str, text: &str) -> Option<String> {
        let path = gsttt_core::RepoPath::normalize(path).unwrap();
        blanked_code(&SourceFile {
            id: gsttt_core::ids::FileId::from_raw(0),
            path: &path,
            text,
        })
    }

    #[test]
    fn comments_and_imports_are_blanked_with_their_delimiters_and_lines_are_kept() {
        let text = "// line
import {
  a,
} from './a'; const x = 1; /* block
spans */ const y = 2;
/** doc */
export const z = a; // tail
";
        let code = blanked("a.ts", text).unwrap();
        assert_eq!(code.matches('\n').count(), text.matches('\n').count());
        let lines: Vec<&str> = code.lines().map(str::trim).collect();
        assert_eq!(
            lines,
            [
                "",
                "",
                "",
                "const x = 1;",
                "const y = 2;",
                "",
                "export const z = a;"
            ]
        );
    }

    #[test]
    fn whitespace_inside_a_template_is_kept_visible_and_lines_stay_where_they_are() {
        let text = "const q = `a\n  b\n\n\tc`;  \n";
        let code = blanked("a.ts", text).unwrap();
        assert_eq!(code.matches('\n').count(), text.matches('\n').count());
        let lines: Vec<&str> = code.lines().map(str::trim).collect();
        assert_eq!(
            lines,
            ["const q = `a\x03", "\x01\x01b\x03", "\x03", "\x02c`;"]
        );
    }

    #[test]
    fn a_text_the_parser_cannot_go_on_with_has_no_code() {
        assert_eq!(blanked("a.ts", "const = = ;;\n))((\n"), None);
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
