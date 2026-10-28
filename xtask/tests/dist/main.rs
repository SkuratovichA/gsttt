//! Tests of the release and repository tasks (`openspec/changes/add-distribution/`). They run the `xtask` binary, and the scripts and workflows of the repository, on small repositories made in a temporary directory.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap, and clippy only knows that inside functions marked as tests, not in the helpers beside them"
)]

mod check;
mod ci;
mod install;
mod notes;
mod package;
mod plugin;
mod root;
mod supply_chain;
mod support;
mod version;
