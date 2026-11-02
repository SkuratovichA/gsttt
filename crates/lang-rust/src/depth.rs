//! How deep the parser would nest a file, measured before any tree is built.

use ra_ap_edition::Edition;
use ra_ap_parser::{LexedStr, Step, SyntaxKind, TopEntryPoint};

/// The most levels a syntax tree may nest. The tree is dropped by a thread that nothing here sizes, and the node cache of the tree hashes the whole subtree of a node, so a tree `n` deep costs `n` squared and an unbounded one aborts the process. Real code is a few dozen levels deep; a chain of a thousand operators is generated code.
pub(crate) const MAX_DEPTH: usize = 2000;

/// The byte offset where the syntax tree of `text` first nests past [`MAX_DEPTH`], and `None` when it never does. It runs the parser without building the tree, which costs a fraction of the parse.
pub(crate) fn too_deep_at(text: &str, edition: Edition) -> Option<usize> {
    // A tree `n` levels deep has at least `n` tokens.
    if text.len() <= MAX_DEPTH {
        return None;
    }
    let lexed = LexedStr::new(edition, text);
    // Every open bracket is a level of the tree, so the lexer alone refuses a file of open brackets before the parser runs a million levels deep.
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
    let output = TopEntryPoint::SourceFile.parse(&lexed.to_input(edition));
    // The parser sees no trivia, so its tokens are counted off the lexer's tokens that are not.
    let mut tokens = (0..lexed.len()).filter(|&token| !lexed.kind(token).is_trivia());
    let mut depth = 0usize;
    for step in output.iter() {
        match step {
            Step::Enter { .. } => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Some(
                        tokens
                            .next()
                            .map_or(text.len(), |token| lexed.text_start(token)),
                    );
                }
            }
            Step::Exit => depth = depth.saturating_sub(1),
            Step::Token { n_input_tokens, .. } => {
                for _ in 0..n_input_tokens {
                    tokens.next();
                }
            }
            Step::FloatSplit { .. } => {
                tokens.next();
            }
            Step::Error { .. } => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use ra_ap_edition::Edition;

    use super::{MAX_DEPTH, too_deep_at};

    /// `too_deep_at` on a thread with the stack the engine grows for a file, which a test thread does not have.
    fn on_a_grown_stack(text: String) -> Option<usize> {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || too_deep_at(&text, Edition::Edition2021))
            .unwrap()
            .join()
            .unwrap()
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
