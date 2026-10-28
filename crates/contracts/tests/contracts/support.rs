//! Shared pieces of the connector tests: fixture trees, a pipeline run over one, the answers the tests read out
//! of it, and a host connector that embeds documents the way a language connector does.

use std::collections::BTreeMap;

use gsttt_contracts::{ENV, GRAPHQL, OPENAPI, PRISMA, PROTO, SQL};
use gsttt_core::connector::{
    Connector, ConnectorError, DynConnector, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::Role;
use gsttt_core::engine::pipeline::{self, FileRun, Module, Output};
use gsttt_core::fact::{
    ContractFact, Embedded, FileFacts, Import, ParseStatus, Resolution, UnresolvedReason,
};
use gsttt_core::ids::Id;
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The size limit a run reads files up to; the fixtures are far below it.
const MAX_FILE_BYTES: u64 = 1 << 20;

/// The six contract connectors in registry order.
pub(crate) fn connectors() -> [&'static dyn DynConnector; 6] {
    [&OPENAPI, &GRAPHQL, &PROTO, &PRISMA, &SQL, &ENV]
}

/// A module that runs a connector with its default options.
pub(crate) fn module(connector: &'static dyn DynConnector) -> Module<'static> {
    with_options(connector, Value::Null)
}

/// A module that runs a connector with these options.
pub(crate) fn with_options(
    connector: &'static dyn DynConnector,
    options: Value,
) -> Module<'static> {
    Module {
        connector,
        options,
        packs: Vec::new(),
    }
}

/// All six contract connectors with their default options.
pub(crate) fn all_modules() -> Vec<Module<'static>> {
    connectors().into_iter().map(module).collect()
}

/// The view of the fixture tree `tests/fixtures/<name>`.
pub(crate) fn fixture(name: &str) -> RepoView {
    RepoView::scan(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// The pipeline over a view, for a test that expects the options to be refused.
pub(crate) fn try_run(repo: &RepoView, modules: &[Module<'_>]) -> Result<Output, ConnectorError> {
    pipeline::run(repo, modules, MAX_FILE_BYTES)
}

/// A pipeline run over a fixture tree, with the questions the tests ask of it.
pub(crate) struct Run {
    pub(crate) repo: RepoView,
    pub(crate) output: Output,
}

impl Run {
    /// Runs `modules` over the fixture tree `name`.
    pub(crate) fn new(name: &str, modules: &[Module<'_>]) -> Self {
        let repo = fixture(name);
        let output = try_run(&repo, modules).unwrap();
        Self { repo, output }
    }

    /// What the run made of a file; the file must be in the tree and taken by a project.
    pub(crate) fn file(&self, path: &str) -> &FileRun {
        let id = self.repo.lookup(path).unwrap();
        self.output.files.iter().find(|file| file.id == id).unwrap()
    }

    /// The parse status of a file.
    pub(crate) fn status(&self, path: &str) -> &ParseStatus {
        &self.file(path).facts.status
    }

    /// The name of the connector whose project took a file.
    pub(crate) fn project_of(&self, path: &str) -> &'static str {
        let project = self.file(path).project.index();
        self.output.projects[project].connector.name
    }

    /// The contract sites of a file as `<role> <key> @<line>`.
    pub(crate) fn sites(&self, path: &str) -> Vec<String> {
        describe(&self.file(path).facts.contracts)
    }

    /// The embedded documents of a file: who read each one, and the sites it gave.
    pub(crate) fn embedded(&self, path: &str) -> Vec<(Option<&'static str>, Vec<String>)> {
        self.file(path)
            .embedded
            .iter()
            .map(|run| (run.read_by, describe(&run.facts)))
            .collect()
    }

    /// The imports of a file as `(spec, where it points)`.
    pub(crate) fn imports(&self, path: &str) -> Vec<(String, String)> {
        self.file(path)
            .imports
            .iter()
            .map(|resolved| (resolved.import.spec.clone(), self.target(&resolved.to)))
            .collect()
    }

    /// The sites that `link` found, as `<path>: <role> <key> @<line>`.
    pub(crate) fn linked(&self) -> Vec<String> {
        let mut linked: Vec<(String, u32, String)> = self
            .output
            .linked
            .iter()
            .map(|linked| {
                let path = self.repo.path(linked.file).unwrap().as_str().to_owned();
                (path, linked.fact.line, site(&linked.fact))
            })
            .collect();
        linked.sort();
        linked
            .into_iter()
            .map(|(path, _, site)| format!("{path}: {site}"))
            .collect()
    }

    /// The files no project took.
    pub(crate) fn unclaimed(&self) -> Vec<String> {
        self.output
            .unclaimed
            .iter()
            .map(|&id| self.repo.path(id).unwrap().as_str().to_owned())
            .collect()
    }

    /// The whole output as JSON for a snapshot, without the provides of each project: every run repeats them, and
    /// the scenario about them keeps them.
    pub(crate) fn snapshot(&self) -> Value {
        let mut json = self.output.to_json(&self.repo);
        if let Some(Value::Array(projects)) = json.get_mut("projects") {
            for project in projects {
                if let Value::Object(map) = project {
                    map.remove("provides");
                }
            }
        }
        sorted(json)
    }

    fn target(&self, to: &Resolution) -> String {
        match to {
            Resolution::File { file } => self.repo.path(*file).unwrap().as_str().to_owned(),
            Resolution::Builtin => "builtin".to_owned(),
            Resolution::Unresolved(UnresolvedReason::UnknownPackage) => {
                "unknown package".to_owned()
            }
            other => format!("{other:?}"),
        }
    }
}

/// The same JSON with the keys of every object in alphabetical order. Another crate of the workspace may turn on
/// the `preserve_order` feature of `serde_json`, which keeps keys in the order they were built in and would change
/// every snapshot depending on which crates are compiled together.
pub(crate) fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<String, Value> = map
                .into_iter()
                .map(|(key, value)| (key, sorted(value)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

fn site(fact: &ContractFact) -> String {
    format!("{} {} @{}", role(fact.role), fact.key, fact.line)
}

fn role(role: Role) -> &'static str {
    match role {
        Role::Declares => "declares",
        Role::Serves => "serves",
        Role::Calls => "calls",
    }
}

/// Contract sites as `<role> <key> @<line>`, by line and then by text.
pub(crate) fn describe(facts: &[ContractFact]) -> Vec<String> {
    let mut sites: Vec<(u32, String)> = facts.iter().map(|fact| (fact.line, site(fact))).collect();
    sites.sort();
    sites.into_iter().map(|(_, site)| site).collect()
}

/// A stand-in for a language connector: it claims `.ts` and `.py` files and reads each line `embed <lang> <text>`
/// as a document of that language at that line, which is all the pipeline needs from a host. A module crate does
/// not depend on another module crate, so the real hosts cannot stand here.
pub(crate) struct Host;

/// The host takes no options.
#[derive(Default, Serialize, Deserialize, JsonSchema)]
pub(crate) struct HostOptions {}

/// The host's only project, at the root.
pub(crate) struct HostProject;

/// The host connector.
pub(crate) static HOST: Erased<Host> = Erased(Host);

impl Connector for Host {
    type Options = HostOptions;
    type Project = HostProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "host",
            version: 1,
            summary: "a host of embedded documents, for tests",
            extensions: &["ts", "py"],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        true
    }

    fn open(
        &self,
        _: &RepoView,
        _: &HostOptions,
        _: &[Pack],
    ) -> Result<Vec<HostProject>, ConnectorError> {
        Ok(vec![HostProject])
    }
}

impl Project for HostProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        matches!(path.extension(), Some("ts" | "py"))
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        Vec::new()
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        let embedded = file
            .text
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let (lang, text) = line.strip_prefix("embed ")?.split_once(' ')?;
                Some(Embedded {
                    lang: lang.to_owned(),
                    line: u32::try_from(index + 1).ok()?,
                    text: text.to_owned(),
                    via: "host".to_owned(),
                })
            })
            .collect();
        Parsed::new(FileFacts {
            embedded,
            ..FileFacts::default()
        })
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::NotFound)
    }
}
