//! The text of a Rust file with its comments and `use` items blanked, which two versions of a file are compared on.

use gsttt_core::connector::SourceFile;
use gsttt_core::text::{blank_spans, mark_whitespace};
use ra_ap_edition::Edition;
use ra_ap_syntax::{NodeOrToken, SyntaxKind, WalkEvent};

use crate::depth;

/// The text of `file` with every comment, doc comments included, and every `use` item blanked, each byte but a line break becoming a space, so that the lines are the lines of the file. What a `use` binds is compared as the facts of the file say, not as text. Whitespace inside a string is part of its value, so there a space becomes `\x01`, a tab `\x02` and a line break is preceded by `\x03`. `None` for a file that is not Rust source, that nests too deep or that has a syntax error, whose tokens cannot be told apart from a comment with certainty.
pub(crate) fn blanked(file: &SourceFile<'_>, edition: Edition) -> Option<String> {
    if file.path.extension() != Some("rs") {
        return None;
    }
    let parse = depth::parse(file.text, edition).ok()?;
    if !parse.errors().is_empty() {
        return None;
    }
    let mut blank = Vec::new();
    let mut strings = Vec::new();
    let mut events = parse.syntax_node().preorder_with_tokens();
    while let Some(event) = events.next() {
        let WalkEvent::Enter(element) = event else {
            continue;
        };
        let range = element.text_range();
        let span = usize::from(range.start())..usize::from(range.end());
        match element {
            NodeOrToken::Node(node) if node.kind() == SyntaxKind::USE => {
                blank.push(span);
                events.skip_subtree();
            }
            NodeOrToken::Token(token) => match token.kind() {
                SyntaxKind::COMMENT | SyntaxKind::DOC_COMMENT => blank.push(span),
                SyntaxKind::STRING
                | SyntaxKind::BYTE_STRING
                | SyntaxKind::C_STRING
                | SyntaxKind::CHAR
                | SyntaxKind::BYTE => strings.push((span.start, span.end)),
                _ => {}
            },
            NodeOrToken::Node(_) => {}
        }
    }
    let bytes = blank_spans(file.text, blank)?;
    mark_whitespace(&bytes, &strings, &[])
}
