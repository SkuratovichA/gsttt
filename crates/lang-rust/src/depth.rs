//! Parses a Rust file, and refuses one that nests too deep before the tree of it costs anything.

use ra_ap_edition::Edition;
use ra_ap_parser::{LexedStr, StrStep, SyntaxKind, TopEntryPoint};
use ra_ap_syntax::{Parse, SyntaxNode, SyntaxTreeBuilder, TextSize};

/// The most levels a syntax tree may nest. The node cache of the tree hashes the whole subtree of a node, so a tree `n` deep costs `n` squared. Real code is a few dozen levels deep; a chain of a thousand operators is generated code.
pub(crate) const MAX_DEPTH: usize = 2000;

/// The syntax tree of `text`, or the byte offset where it first nests past [`MAX_DEPTH`]. It is `ast::SourceFile::parse` with the levels counted: the tree is built from the steps they are counted on, so a file is lexed and parsed once, and nothing is built past the limit.
pub(crate) fn parse(text: &str, edition: Edition) -> Result<Parse<SyntaxNode>, usize> {
    let lexed = LexedStr::new(edition, text);
    if let Some(offset) = too_deep_for_the_parser(&lexed) {
        return Err(offset);
    }
    let output = TopEntryPoint::SourceFile.parse(&lexed.to_input(edition));
    let mut tree = SyntaxTreeBuilder::default();
    let mut depth = 0usize;
    let mut offset = 0usize;
    let mut too_deep = false;
    lexed.intersperse_trivia(&output, &mut |step| {
        if too_deep {
            return;
        }
        match step {
            StrStep::Enter { kind } => {
                depth += 1;
                too_deep = depth > MAX_DEPTH;
                if !too_deep {
                    tree.start_node(kind);
                }
            }
            StrStep::Exit => {
                depth = depth.saturating_sub(1);
                tree.finish_node();
            }
            StrStep::Token { kind, text } => {
                offset += text.len();
                tree.token(kind, text);
            }
            StrStep::Error { msg, pos } => tree.error(msg.to_owned(), text_size(pos)),
        }
    });
    if too_deep {
        return Err(offset);
    }
    for (token, message) in lexed.errors() {
        tree.error(message.to_owned(), text_size(lexed.text_start(token)));
    }
    Ok(tree.finish())
}

/// A position in a text, which the lexer has already kept in 32 bits.
fn text_size(offset: usize) -> TextSize {
    TextSize::try_from(offset).unwrap_or(TextSize::new(u32::MAX))
}

/// The byte offset where the brackets or the prefix operators of a file pass [`MAX_DEPTH`]. Each of them is a level of the tree and a level of the recursion of the parser, so the tokens alone refuse a file that would take the parser that deep.
fn too_deep_for_the_parser(lexed: &LexedStr<'_>) -> Option<usize> {
    let mut open = 0usize;
    // A run of prefix operators nests one level each, as `- - - 1` does.
    let mut prefixes = 0usize;
    for token in 0..lexed.len() {
        let kind = lexed.kind(token);
        if kind.is_trivia() {
            continue;
        }
        if matches!(
            kind,
            SyntaxKind::MINUS | SyntaxKind::BANG | SyntaxKind::STAR | SyntaxKind::AMP
        ) {
            prefixes += 1;
            if prefixes > MAX_DEPTH {
                return Some(lexed.text_start(token));
            }
        } else {
            prefixes = 0;
        }
        match kind {
            SyntaxKind::L_PAREN | SyntaxKind::L_BRACK | SyntaxKind::L_CURLY => {
                open += 1;
                if open > MAX_DEPTH {
                    return Some(lexed.text_start(token));
                }
            }
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACK | SyntaxKind::R_CURLY => {
                open = open.saturating_sub(1);
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::{SyntaxError, TextSize, ast};

    use super::{MAX_DEPTH, parse};

    /// Where `parse` refuses `text`, on a thread with the stack the engine grows for a file, which a test thread does not have.
    fn on_a_grown_stack(text: String) -> Option<usize> {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || parse(&text, Edition::Edition2021).err())
            .unwrap()
            .join()
            .unwrap()
    }

    fn messages(errors: &[SyntaxError]) -> Vec<(String, TextSize)> {
        errors
            .iter()
            .map(|error| (error.to_string(), error.range().start()))
            .collect()
    }

    /// The same tree and the same errors as the parser of the library: `ra_ap_syntax` keeps the function that builds its tree private, so `parse` holds a copy of it.
    fn assert_same_as_the_library(text: &str) {
        let ours = parse(text, Edition::Edition2021).unwrap();
        let theirs = ast::SourceFile::parse(text, Edition::Edition2021);
        assert_eq!(
            format!("{:#?}", ours.syntax_node()),
            format!("{:#?}", theirs.syntax_node()),
        );
        assert_eq!(messages(&ours.errors()), messages(&theirs.errors()));
    }

    #[test]
    fn the_tree_and_the_errors_are_those_of_the_parser_of_the_library() {
        for text in [
            "",
            "// Nothing but a comment.\n",
            "/// A comment that attaches to the item.\nfn f() { let x = t.0.1 + 1.5; }\n",
            "fn f( { let = ; }\n",
            "fn f() { let s = \"cut off; }\n",
            "#[a] mod m { 'x }\n",
        ] {
            assert_same_as_the_library(text);
        }
    }

    proptest! {
        #[test]
        fn any_text_gives_the_tree_and_the_errors_of_the_parser_of_the_library(
            text in r#"(fn |let |mod |[a-z]+|[0-9]\.[0-9]|\.|[(){}\[\]]|[-!*&+=;,:]|"|'|//[a-z ]*\n| |\n){0,40}"#,
        ) {
            assert_same_as_the_library(&text);
        }
    }

    #[test]
    fn code_that_is_long_and_shallow_is_not_too_deep() {
        let text = "fn f() { let x = (1 + (2 * 3)); }\n".repeat(MAX_DEPTH);
        assert_eq!(on_a_grown_stack(text), None);
    }

    #[test]
    fn a_tree_past_the_limit_is_found_at_the_level_that_passes_it() {
        let text = format!("const X: i32 = {}1;\n", "(".repeat(MAX_DEPTH));
        let len = text.len();
        let offset = on_a_grown_stack(text);
        assert!(offset.is_some_and(|offset| offset > 0 && offset < len));
    }

    #[test]
    fn a_sum_that_nests_to_the_left_is_found_by_the_parser() {
        let text = format!("const X: i32 = 1{};\n", " + 1".repeat(MAX_DEPTH + 1));
        assert!(on_a_grown_stack(text).is_some());
    }

    #[test]
    fn a_run_of_prefix_operators_is_found_without_the_parser() {
        let text = format!("const X: i32 = {}1;\n", "! ".repeat(MAX_DEPTH + 1));
        assert!(on_a_grown_stack(text).is_some());
    }
}
