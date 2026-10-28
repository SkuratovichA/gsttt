//! `#[scenario("<capability>", "<scenario>")]`: names the OpenSpec scenario a test proves.
//!
//! The attribute changes nothing about the function it marks; `cargo xtask spec` reads it from the source files and checks that every scenario has exactly one proof. The checking of the arguments lives in `gsttt-spec`, because a proc-macro crate can export nothing but macros and the gate has to read the arguments by the same rule.

use proc_macro::TokenStream;

/// Marks a test as the proof of one scenario of an OpenSpec spec: the capability (the directory under `openspec/specs/`) and the scenario title, exactly as written after `#### Scenario:`.
///
/// ```ignore
/// #[scenario("queries", "Impact goes through wormholes")]
/// #[test]
/// fn impact_crosses_wormholes() { /* … */ }
/// ```
///
/// The function is left as it is. Any arguments but two string literals are a compile error at the arguments.
#[proc_macro_attribute]
pub fn scenario(args: TokenStream, item: TokenStream) -> TokenStream {
    gsttt_spec::expand_scenario(args.into(), item.into()).into()
}
