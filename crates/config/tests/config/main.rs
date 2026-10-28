//! One test per scenario of the `config` capability (`openspec/specs/config/spec.md`).

// The helpers unwrap like the tests that call them; clippy only knows that for code under `cfg(test)`.
#[cfg(test)]
mod editor;
mod evaluation;
#[cfg(test)]
mod external;
#[cfg(test)]
mod fakes;
#[cfg(test)]
mod nesting;
#[cfg(test)]
mod packs;
#[cfg(test)]
mod source;
#[cfg(test)]
mod tree;
