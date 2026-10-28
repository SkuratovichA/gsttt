//! The arguments of `#[scenario]` and what the attribute expands to.

use gsttt_spec::{expand_scenario, scenario_args};
use proc_macro2::{LineColumn, TokenStream, TokenTree};

const ITEM: &str = "fn marked() {}";

#[expect(
    clippy::unwrap_used,
    reason = "a test helper: source that does not lex should stop the test"
)]
fn tokens(source: &str) -> TokenStream {
    source.parse().unwrap()
}

/// Where the `compile_error!` that an expansion starts with points.
fn error_start(expanded: &TokenStream) -> Option<LineColumn> {
    expanded.clone().into_iter().find_map(|tree| match tree {
        TokenTree::Ident(ident) if ident == "compile_error" => Some(ident.span().start()),
        _ => None,
    })
}

// The compiler's view of the same cases, through the attribute itself, is `gsttt-scenario`'s
// `tests/ui.rs`, which proves the scenario.
#[test]
fn wrong_arguments_expand_to_an_error_at_the_argument() {
    // The arguments and the column the error must point at: the offending argument, or the first
    // one when the count is wrong.
    let cases = [
        (r#""only-one""#, 0),
        (r#"capability, "x""#, 0),
        (r#""a", 7"#, 5),
        (r#""a" "b""#, 4),
        (r#""a", "b", "c""#, 0),
    ];
    for (args, column) in cases {
        let expanded = expand_scenario(tokens(args), tokens(ITEM));
        assert_eq!(
            error_start(&expanded),
            Some(LineColumn { line: 1, column }),
            "{args}"
        );
        assert!(
            expanded.to_string().ends_with(&tokens(ITEM).to_string()),
            "the marked item is kept so that nothing else fails: {expanded}"
        );
    }
}

#[test]
fn missing_arguments_do_not_compile() {
    let expanded = expand_scenario(TokenStream::new(), tokens(ITEM));
    assert!(error_start(&expanded).is_some(), "{expanded}");
    assert!(
        expanded
            .to_string()
            .contains("expected two string literals")
    );
}

#[test]
fn valid_arguments_leave_the_function_unchanged() {
    for args in [
        r#""queries", "HTTP matching is symmetric""#,
        r#""queries", "Defaults apply","#,
        r##""queries", r#"A "quoted" title"#"##,
    ] {
        let expanded = expand_scenario(tokens(args), tokens(ITEM));
        assert_eq!(expanded.to_string(), tokens(ITEM).to_string(), "{args}");
    }
}

#[test]
fn the_arguments_are_the_capability_and_the_title() {
    let (capability, title) =
        scenario_args(tokens(r##""queries", r#"A "quoted" title"#"##)).unwrap();
    assert_eq!(capability, "queries");
    assert_eq!(title, r#"A "quoted" title"#);
}
