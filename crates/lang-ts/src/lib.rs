//! TypeScript and JavaScript for gsttt: one project per package, imports of every form, resolution over the
//! repository's own file list, and rule packs for the frameworks whose code carries contracts.

mod analyze;
mod builtins;
mod config;
mod connector;
pub mod extra;
mod json;
mod link;
mod manifest;
mod options;
mod pathing;
mod project;
mod provides;
mod resolve;
mod shape;
mod shared;
mod source;

pub use connector::{PACKS, TYPESCRIPT, TypeScript};
pub use extra::TsExtra;
pub use options::TypeScriptOptions;
pub use project::TsProject;
