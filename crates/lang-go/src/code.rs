//! The text of a Go file with its comments and imports blanked and the alignment of gofmt squeezed, which two versions of a file are compared on.

use gsttt_core::connector::SourceFile;
use gsttt_core::fact::ParseStatus;
use gsttt_core::text::{blank_spans, mark_whitespace};
use tree_sitter::Node;

use crate::files::is_manifest;
use crate::syntax::{parse_tree, scan, text_of};

/// What the walk of the tree finds: the byte ranges to blank and the byte ranges whose whitespace is part of the program.
#[derive(Default)]
struct Spans {
    blanked: Vec<(usize, usize)>,
    values: Vec<(usize, usize)>,
    /// Where the comments directly above an `import "C"` start: C code, not commentary.
    preamble: Vec<usize>,
}

/// The text of `file` with every comment and every import declaration blanked, each byte but a line break becoming a space, so that the lines are the lines of the file, and with every run of spaces and tabs squeezed to one space, because gofmt realigns a whole block when one line of it grows. Whitespace inside a string or a rune is marked and stays. A `//go:` directive, a `// +build` line and the preamble of cgo are program and stay. `None` for a file that is no Go source or has a syntax error, whose tokens cannot be told apart from a comment with certainty.
pub(crate) fn blanked(file: &SourceFile<'_>) -> Option<String> {
    if is_manifest(file.path) {
        return None;
    }
    let tree = parse_tree(file.text)?;
    let root = tree.root_node();
    let found = scan(root);
    if found.too_deep.is_some() || !matches!(found.errors.status(), ParseStatus::Ok) {
        return None;
    }
    let spans = spans_of(root, file.text);
    let bytes = blank_spans(
        file.text,
        spans.blanked.iter().map(|&(start, end)| start..end),
    )?;
    Some(squeezed(&mark_whitespace(&bytes, &spans.values, &[])?))
}

/// Walks the tree in document order without recursion. A comment, an import declaration and a literal are leaves of the walk, so no range lies inside another.
fn spans_of(root: Node<'_>, source: &str) -> Spans {
    let mut spans = Spans::default();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let range = (node.start_byte(), node.end_byte());
        let descend = match node.kind() {
            "comment" => {
                if !is_directive(text_of(node, source)) {
                    spans.blanked.push(range);
                }
                false
            }
            "import_declaration" => {
                if imports_c(node, source) {
                    preamble_of(node, &mut spans.preamble);
                }
                spans.blanked.push(range);
                false
            }
            "interpreted_string_literal" | "raw_string_literal" | "rune_literal" => {
                spans.values.push(range);
                false
            }
            _ => true,
        };
        if descend && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                spans
                    .blanked
                    .retain(|&(start, _)| spans.preamble.binary_search(&start).is_err());
                return spans;
            }
        }
    }
}

/// Whether a comment changes what compiles or what a variable holds: a build constraint, `//go:embed`, `//go:generate`, `//go:linkname`.
fn is_directive(comment: &str) -> bool {
    comment.starts_with("//go:") || comment.starts_with("// +build")
}

/// Whether the declaration imports the pseudo package `"C"`, alone or in a group.
fn imports_c(declaration: Node<'_>, source: &str) -> bool {
    let mut cursor = declaration.walk();
    let mut stack: Vec<Node<'_>> = declaration.named_children(&mut cursor).collect();
    while let Some(node) = stack.pop() {
        if node.kind() == "import_spec" {
            let path = node.child_by_field_name("path");
            if path.is_some_and(|path| text_of(path, source) == "\"C\"") {
                return true;
            }
        } else {
            let mut inner = node.walk();
            stack.extend(node.named_children(&mut inner));
        }
    }
    false
}

/// Notes the start of every comment directly above `declaration`. The list ends up sorted, since the walk meets them in document order.
fn preamble_of(declaration: Node<'_>, starts: &mut Vec<usize>) {
    let mut above = declaration.prev_sibling();
    let mut found = Vec::new();
    while let Some(node) = above.filter(|node| node.kind() == "comment") {
        found.push(node.start_byte());
        above = node.prev_sibling();
    }
    found.reverse();
    starts.extend(found);
}

/// `marked` with each run of spaces and tabs inside a line turned into one space. The marks of whitespace in a value are other bytes, so they stay. Whitespace between tokens is never removed: `a &^ b` and `a & ^b` are different programs.
fn squeezed(marked: &str) -> String {
    let mut out = String::with_capacity(marked.len());
    let mut after_space = false;
    for ch in marked.chars() {
        let space = matches!(ch, ' ' | '\t');
        if !(space && after_space) {
            out.push(if space { ' ' } else { ch });
        }
        after_space = space;
    }
    out
}
