//! The tree-sitter parse of a Go file, and what is read off the whole tree before anything walks it.

use std::cell::RefCell;

use gsttt_core::fact::{ErrorCount, Line};
use tree_sitter::{Node, Parser, Tree};

/// The most levels a syntax tree may nest. Every walk of the connector recurses once per level, so a file past this is read as far as its nesting and no further: it is generated code, and the stack is not ours to spend on it. The same limit the Rust connector keeps.
pub(crate) const MAX_DEPTH: usize = 2000;

thread_local! {
    /// A parser is not `Sync` and costs a grammar load, so each thread of the engine keeps one.
    static PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
}

/// The syntax tree of `text`, or `None` when the grammar cannot be loaded or the parser gives up.
pub(crate) fn parse_tree(text: &str) -> Option<Tree> {
    PARSER.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        if slot.is_none() {
            let mut parser = Parser::new();
            parser.set_language(&tree_sitter_go::LANGUAGE.into()).ok()?;
            *slot = Some(parser);
        }
        slot.as_mut()?.parse(text, None)
    })
}

/// What one pass over the tree finds out.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scan {
    /// The syntax errors of the tree.
    pub(crate) errors: ErrorCount,
    /// The line where the tree first nests past [`MAX_DEPTH`].
    pub(crate) too_deep: Option<Line>,
}

/// Counts the syntax errors of the tree and finds where it nests too deep, without recursion.
pub(crate) fn scan(root: Node<'_>) -> Scan {
    let mut found = Scan::default();
    let mut cursor = root.walk();
    let mut depth = 1usize;
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            found.errors.note(line_of(node));
        }
        if depth > MAX_DEPTH {
            found.too_deep = Some(line_of(node));
            return found;
        }
        // A subtree with no error in it has nothing to count, but its depth still counts.
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
            depth = depth.saturating_sub(1);
        }
    }
}

/// Whether no direct child of the root is a syntax error, so every declaration of the file sits where the grammar put it, even when a function body was recovered.
pub(crate) fn top_level_intact(root: Node<'_>) -> bool {
    let mut cursor = root.walk();
    root.children(&mut cursor)
        .all(|child| !child.is_error() && !child.is_missing())
}

/// The line, from 1, where the node starts.
pub(crate) fn line_of(node: Node<'_>) -> Line {
    Line::try_from(node.start_position().row.saturating_add(1)).unwrap_or(Line::MAX)
}

/// The line, from 1, where the node ends.
pub(crate) fn end_line_of(node: Node<'_>) -> Line {
    Line::try_from(node.end_position().row.saturating_add(1)).unwrap_or(Line::MAX)
}

/// The source text of the node, empty when it is not valid UTF-8 at those bytes.
pub(crate) fn text_of<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
}

/// The text of the child in the field `field`.
pub(crate) fn field_text<'a>(node: Node<'_>, field: &str, source: &'a str) -> Option<&'a str> {
    node.child_by_field_name(field)
        .map(|child| text_of(child, source))
}

/// The named children of the node in the field `field`, in source order.
pub(crate) fn fields<'t>(node: Node<'t>, field: &str) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor)
        .filter(Node::is_named)
        .collect()
}

/// The named children of the node, in source order.
pub(crate) fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
