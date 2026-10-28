//! The arguments of `#[scenario("<capability>", "<scenario>")]` and what the attribute expands to.
//!
//! A proc-macro crate can export nothing but macros, so the logic of `gsttt-scenario` sits here: it
//! can be tested here, and the spec gate reads the arguments of every attribute it finds with the
//! same function, so the compiler and the gate cannot disagree on what a valid attribute is.

use proc_macro2::TokenStream;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::{Error, LitStr, Token};

const USAGE: &str = "expected two string literals: #[scenario(\"<capability>\", \"<scenario>\")]";

/// Reads the arguments of a `scenario` attribute: the capability and the scenario title, as two
/// string literals.
///
/// # Errors
///
/// Anything else is an error spanned at the offending argument or, when the count is wrong, at all
/// the arguments, ready to become a `compile_error!`.
pub fn scenario_args(args: TokenStream) -> syn::Result<(String, String)> {
    let literals = Punctuated::<LitStr, Token![,]>::parse_terminated
        .parse2(args.clone())
        .map_err(|error| Error::new(error.span(), USAGE))?;
    let mut values = literals.iter().map(LitStr::value);
    match (values.next(), values.next(), values.next()) {
        (Some(capability), Some(scenario), None) => Ok((capability, scenario)),
        _ => Err(Error::new_spanned(args, USAGE)),
    }
}

/// What `#[scenario(args)]` on `item` expands to: the item itself, or, when the arguments are wrong,
/// a `compile_error!` at the arguments followed by the item, so that nothing else fails on its account.
#[must_use]
pub fn expand_scenario(args: TokenStream, item: TokenStream) -> TokenStream {
    match scenario_args(args) {
        Ok(_) => item,
        Err(error) => {
            let mut expanded = error.into_compile_error();
            expanded.extend(item);
            expanded
        }
    }
}
