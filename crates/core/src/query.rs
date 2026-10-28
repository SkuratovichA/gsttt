//! Questions over an [`Index`](crate::index::Index): what a file depends on, what depends on it, what a change affects, where a contract is declared, served and called, and how two files are connected.
//!
//! Each question is a function over `&Index` and a [`Freshness`](crate::index::Freshness), because the index does not know whether the tree has moved since it was built. Each returns an answer that implements [`Display`](std::fmt::Display) for the text people and agents read and [`Serialize`](serde::Serialize) for scripts. Nothing here prints.

mod contracts;
mod cross;
mod http;
mod impact;
mod lookup;
mod neighbours;
mod render;
mod report;
mod route;
mod walk;

pub use contracts::{ContractLookup, ContractLookupError, Found, Match, Site, contract};
pub use http::HttpMatcher;
pub use impact::{Affected, Deleted, Impact, ImpactRequest, Reason, impact};
pub use lookup::Miss;
pub use neighbours::{Dependencies, Dependents, External, Neighbour, dependencies, dependents};
pub use render::{Answer, ConnectorShare, Entries, Footer, LIST_CAP, Searched};
pub use report::{
    ConnectorReport, CoverageReport, Failure, Reasons, Spot, Status, UnclaimedExtension,
    UnclaimedLanguage, coverage, status,
};
pub use route::{Link, Route, Step, path};
