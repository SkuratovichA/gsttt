//! Connectors that stand in for the registry the binary passes in: they have names and option schemas, and no files. Module crates cannot depend on each other, so the real ones are out of reach of this crate's tests.

use std::marker::PhantomData;

use gsttt_core::connector::{
    Connector, ConnectorError, DynConnector, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::fact::{Import, Resolution, UnresolvedReason};
use gsttt_core::rules::Pack;
use gsttt_core::{RepoPath, RepoView};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The options of the stand-in `ts`.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TsOptions {
    /// The tsconfig the project reads, relative to the repository root.
    pub(crate) tsconfig: Option<String>,
    /// Export conditions to follow, in order.
    pub(crate) conditions: Vec<String>,
}

/// The options of a stand-in without any.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoOptions {}

/// A connector with a name and option type `O` that finds nothing.
pub(crate) struct Fake<O> {
    name: &'static str,
    options: PhantomData<fn() -> O>,
}

impl<O> Fake<O> {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            options: PhantomData,
        }
    }
}

impl<O> Connector for Fake<O>
where
    O: Serialize + DeserializeOwned + JsonSchema + Default + Send + Sync + 'static,
{
    type Options = O;
    type Project = Empty;

    fn meta(&self) -> Meta {
        Meta {
            name: self.name,
            version: 1,
            summary: "a stand-in",
            extensions: &[],
        }
    }

    fn detect(&self, _repo: &RepoView) -> bool {
        false
    }

    fn open(
        &self,
        _repo: &RepoView,
        _options: &O,
        _packs: &[Pack],
    ) -> Result<Vec<Empty>, ConnectorError> {
        Ok(Vec::new())
    }
}

/// The project of a stand-in: it owns no file.
pub(crate) struct Empty;

impl Project for Empty {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, _path: &RepoPath) -> bool {
        false
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        Vec::new()
    }

    fn parse(&self, _file: &SourceFile<'_>) -> Parsed<()> {
        Parsed::default()
    }

    fn resolve(&self, _from: &RepoPath, _import: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::NotFound)
    }
}

/// The modules the tests run against: `ts`, `python`, `rust` and `openapi`, in that order.
pub(crate) struct Registry(Vec<Box<dyn DynConnector>>);

impl Registry {
    pub(crate) fn standard() -> Self {
        Self(vec![
            Box::new(Erased(Fake::<TsOptions>::new("ts"))),
            Box::new(Erased(Fake::<NoOptions>::new("python"))),
            Box::new(Erased(Fake::<NoOptions>::new("rust"))),
            Box::new(Erased(Fake::<NoOptions>::new("openapi"))),
        ])
    }

    pub(crate) fn connectors(&self) -> Vec<&dyn DynConnector> {
        self.0.iter().map(AsRef::as_ref).collect()
    }
}
