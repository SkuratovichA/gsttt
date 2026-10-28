//! `#[scenario]`: ties a test to the OpenSpec scenario it proves.

use proc_macro::TokenStream;

/// Marks a test as the proof of an OpenSpec scenario.
#[proc_macro_attribute]
pub fn scenario(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}
