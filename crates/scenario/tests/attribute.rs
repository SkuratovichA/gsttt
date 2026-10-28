//! The attribute on real code: it compiles with valid arguments and leaves the function as it was.

include!("fixtures/marked.rs");

#[test]
fn the_marked_function_is_unchanged() {
    assert_eq!(marked(2, 3), 5);
}
