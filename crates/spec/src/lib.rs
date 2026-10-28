//! Reads OpenSpec files: specs, change deltas and task lists, each item with its file and line, and the arguments of the `#[scenario]` attribute that ties a test to a scenario.
//!
//! The spec gate (`cargo xtask spec`) builds on this crate. The reader follows OpenSpec's own line-and-fence reading (`dist/core/parsers` in the 1.15 CLI) so that the gate and the CLI that archives the specs see the same headings.

mod attribute;
mod capability;
mod directory;
mod error;
mod markdown;
mod tasks;
mod tree;

pub use attribute::{expand_scenario, scenario_args};
pub use capability::{
    Delta, Purpose, Removed, Renamed, Requirement, Scenario, Spec, parse_delta, parse_spec,
};
pub use directory::{Entry, read_directory};
pub use error::{Error, Problem};
pub use tasks::{Task, parse_tasks};
pub use tree::{Change, OpenSpec, read};
