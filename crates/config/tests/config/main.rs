//! One test per scenario of the `config` capability (`openspec/specs/config/spec.md`).

mod editor;
mod evaluation;
// The helpers unwrap like the tests that call them; clippy only knows that for code under `cfg(test)`.
#[cfg(test)]
mod fakes;
mod packs;
#[cfg(test)]
mod tree;
