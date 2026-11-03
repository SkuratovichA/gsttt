//! The text of a Python file with its comments and import statements blanked, which two versions of a file are compared on.

use gsttt_core::connector::SourceFile;
use gsttt_core::text::{blank_spans, mark_whitespace};
use ruff_python_ast::Stmt;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::token::{Token, TokenKind};
use ruff_python_parser::parse_unchecked_source;
use ruff_text_size::{Ranged, TextRange};

use crate::files::{is_python_source, source_type};

/// The ranges of every import statement of a file, nested ones too: what such a statement binds is compared as the facts of the file say, not as text.
#[derive(Default)]
struct ImportRanges(Vec<TextRange>);

impl<'a> StatementVisitor<'a> for ImportRanges {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import) => self.0.push(import.range()),
            Stmt::ImportFrom(import) => self.0.push(import.range()),
            _ => walk_stmt(self, stmt),
        }
    }
}

/// The text of `file` with every comment and every import statement blanked, each byte but a line break becoming a space, so that the lines are the lines of the file. Indentation decides which block a statement belongs to and whitespace in a string is part of its value, so there a space becomes `\x01`, a tab `\x02` and a line break is preceded by `\x03`: a statement that moves to another block, or a blank line in a string, still differs once a line is trimmed. `None` for a file that is not Python source or that has a syntax error, whose tokens cannot be told apart from a comment with certainty.
pub(crate) fn blanked(file: &SourceFile<'_>) -> Option<String> {
    if !is_python_source(file.path) {
        return None;
    }
    let parsed = parse_unchecked_source(file.text, source_type(file.path));
    if !parsed.errors().is_empty() {
        return None;
    }
    let mut imports = ImportRanges::default();
    imports.visit_body(parsed.suite());
    let comments = parsed
        .tokens()
        .iter()
        .filter(|token| token.kind() == TokenKind::Comment)
        .map(Ranged::range);
    let bytes = blank_spans(
        file.text,
        comments
            .chain(imports.0)
            .map(|range| usize::from(range.start())..usize::from(range.end())),
    )?;
    let strings: Vec<(usize, usize)> = parsed
        .tokens()
        .iter()
        .filter(|token| {
            matches!(
                token.kind(),
                TokenKind::String | TokenKind::FStringMiddle | TokenKind::TStringMiddle
            )
        })
        .map(|token| (usize::from(token.start()), usize::from(token.end())))
        .collect();
    let indents = indentation(parsed.tokens().iter(), &bytes);
    mark_whitespace(&bytes, &strings, &indents)
}

/// Where the indentation of each statement that starts a logical line ends and where it begins, in byte offsets.
fn indentation<'a>(tokens: impl Iterator<Item = &'a Token>, bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut fresh = true;
    for token in tokens {
        match token.kind() {
            TokenKind::Newline => fresh = true,
            TokenKind::NonLogicalNewline
            | TokenKind::Comment
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::EndOfFile => {}
            _ => {
                if fresh {
                    let end = usize::from(token.start());
                    let begin = bytes
                        .get(..end)
                        .and_then(|before| {
                            before
                                .iter()
                                .rposition(|&byte| matches!(byte, b'\n' | b'\r'))
                        })
                        .map_or(0, |at| at + 1);
                    starts.push((begin, end));
                }
                fresh = false;
            }
        }
    }
    starts
}
