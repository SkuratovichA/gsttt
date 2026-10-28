//! OpenAPI 3 and Swagger 2 documents, in YAML or JSON.
//!
//! Each operation declares `http <METHOD> <base><path>` at its own line. A path item that is a `$ref` to another file
//! is a fragment: the fragment declares its operations under the path that refers to it, which only the referring
//! file knows, so it is the link step that writes them.

use std::collections::BTreeSet;

use globset::{Glob, GlobSet, GlobSetBuilder};
use gsttt_core::connector::{
    Connector, ConnectorError, Erased, LinkInput, LinkedFact, Meta, Parsed, Project, Provide,
    SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{ContractFact, FileFacts, Import, ImportKind, Line, Resolution};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{
    SyntaxError, any_claimed, http_site, provide, resolve_relative, syntax_failure,
};
use crate::tree::{Value, parse_json, parse_yaml};

const VIA: &str = "openapi";

/// The methods a path item can hold, as written and as keys spell them.
const METHODS: [(&str, &str); 8] = [
    ("get", "GET"),
    ("put", "PUT"),
    ("post", "POST"),
    ("delete", "DELETE"),
    ("options", "OPTIONS"),
    ("head", "HEAD"),
    ("patch", "PATCH"),
    ("trace", "TRACE"),
];

/// Documents larger than this are not read when looking for the fragments a `$ref` reaches.
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

/// The connector for OpenAPI and Swagger documents.
#[derive(Clone, Copy, Debug)]
pub struct OpenApi;

/// The connector for OpenAPI and Swagger documents.
pub static OPENAPI: Erased<OpenApi> = Erased(OpenApi);

/// The options of the `openapi` connector.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct OpenApiOptions {
    /// Globs, relative to the repository root, of files to read as OpenAPI documents besides the ones named
    /// `openapi.*`, `swagger.*` and `*.openapi.*`.
    pub files: Vec<String>,
}

/// The project of the `openapi` connector.
#[derive(Clone, Debug)]
pub struct OpenApiProject {
    repo: RepoView,
    globs: GlobSet,
    /// The files that `$ref`s of the documents reach, transitively.
    fragments: BTreeSet<String>,
}

/// What a file hands to the link step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiExtra {
    /// The path items of a document that are a `$ref` to a fragment.
    path_refs: Vec<PathRef>,
    /// The operations written at the top level of a fragment.
    operations: Vec<Operation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PathRef {
    /// The file the `$ref` names, as written, to find the import it became.
    spec: String,
    line: Line,
    /// The route the path item stands for, server base included.
    route: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Operation {
    method: String,
    line: Line,
}

/// `openapi.*`, `swagger.*` and `*.openapi.*` with a YAML or JSON extension.
fn claims_by_name(path: &RepoPath) -> bool {
    if !matches!(path.extension(), Some("yaml" | "yml" | "json")) {
        return false;
    }
    let name = path.file_name();
    name.starts_with("openapi.") || name.starts_with("swagger.") || name.contains(".openapi.")
}

impl Connector for OpenApi {
    type Options = OpenApiOptions;
    type Project = OpenApiProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "openapi",
            version: 1,
            summary: "HTTP routes declared by OpenAPI and Swagger documents",
            extensions: &["yaml", "yml", "json"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        any_claimed(repo, claims_by_name)
    }

    fn open(
        &self,
        repo: &RepoView,
        options: &OpenApiOptions,
        _: &[Pack],
    ) -> Result<Vec<OpenApiProject>, ConnectorError> {
        let globs = build_globs(&options.files)?;
        let documents: Vec<&RepoPath> = repo
            .files()
            .map(|(_, path)| path)
            .filter(|path| claims_by_name(path) || globs.is_match(path.as_str()))
            .collect();
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![OpenApiProject {
            repo: repo.clone(),
            globs,
            fragments: fragments_of(repo, &documents),
        }])
    }
}

fn build_globs(patterns: &[String]) -> Result<GlobSet, ConnectorError> {
    let options_error = |message: String| ConnectorError::Options {
        connector: "openapi".into(),
        message,
    };
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            options_error(format!(
                "`files` has the glob {pattern:?}, which is not valid: {error}"
            ))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|error| options_error(format!("`files` does not build: {error}")))
}

/// The YAML and JSON files that the `$ref`s of the documents reach, and those of the files they reach in turn.
fn fragments_of(repo: &RepoView, documents: &[&RepoPath]) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = documents
        .iter()
        .map(|path| path.as_str().to_owned())
        .collect();
    let mut queue: Vec<RepoPath> = documents.iter().map(|path| (*path).clone()).collect();
    let mut fragments = BTreeSet::new();
    while let Some(path) = queue.pop() {
        let Ok(text) = repo.read_to_string(path.as_str()) else {
            continue;
        };
        if text.len() > MAX_DOCUMENT_BYTES {
            continue;
        }
        let Ok(read) = read(&path, &text) else {
            continue;
        };
        for import in read.imports {
            let Ok(target) = RepoPath::join(path.parent(), &import.spec) else {
                continue;
            };
            let reachable = repo.lookup(target.as_str()).is_some()
                && matches!(target.extension(), Some("yaml" | "yml" | "json"));
            if reachable && seen.insert(target.as_str().to_owned()) {
                fragments.insert(target.as_str().to_owned());
                queue.push(target);
            }
        }
    }
    fragments
}

impl Project for OpenApiProject {
    type Extra = OpenApiExtra;

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        claims_by_name(path)
            || self.globs.is_match(path.as_str())
            || self.fragments.contains(path.as_str())
    }

    fn salt(&self) -> u64 {
        // The `files` patterns are an option, and the options are part of the key of every fact.
        0
    }

    fn provides(&self) -> Vec<Provide> {
        vec![provide(
            ContractKind::Http,
            Role::Declares,
            &["OpenAPI 3 and Swagger 2 operations"],
            &[
                "schemas generated at build time",
                "path items that refer to another place in the same document",
            ],
        )]
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<OpenApiExtra> {
        match read(file.path, file.text) {
            Ok(read) => Parsed {
                facts: FileFacts {
                    imports: read.imports,
                    contracts: read.facts,
                    ..FileFacts::default()
                },
                extra: read.extra,
            },
            Err(error) => Parsed::new(syntax_failure(error.line, error.message)),
        }
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        resolve_relative(&self.repo, from, &import.spec)
    }

    fn link(&self, input: &LinkInput<'_, OpenApiExtra>) -> Vec<LinkedFact> {
        let mut linked = Vec::new();
        for file in input.files() {
            for path_ref in &file.extra.path_refs {
                let target = file
                    .imports
                    .iter()
                    .find(|resolved| {
                        resolved.import.spec == path_ref.spec
                            && resolved.import.line == path_ref.line
                    })
                    .and_then(|resolved| match &resolved.to {
                        Resolution::File { file } => Some(*file),
                        _ => None,
                    });
                let Some(fragment) = target.and_then(|id| input.get(id)) else {
                    continue;
                };
                for operation in &fragment.extra.operations {
                    linked.push(LinkedFact {
                        file: fragment.id,
                        fact: http_site(
                            &operation.method,
                            &path_ref.route,
                            Role::Declares,
                            operation.line,
                            VIA,
                        ),
                    });
                }
            }
        }
        linked
    }
}

/// What one document says.
#[derive(Debug, Default)]
struct Read {
    facts: Vec<ContractFact>,
    imports: Vec<Import>,
    extra: OpenApiExtra,
}

fn read(path: &RepoPath, text: &str) -> Result<Read, SyntaxError> {
    let root = if path.extension() == Some("json") {
        parse_json(text)?
    } else {
        parse_yaml(text)?
    };
    let mut read = Read::default();
    collect_refs(&root, &mut read.imports);
    if ["openapi", "swagger", "paths"]
        .iter()
        .any(|key| root.get(key).is_some())
    {
        read_document(&root, &mut read);
    } else {
        // A document with none of those keys is a path item on its own: a fragment another document refers to.
        for entry in root.entries() {
            if let Some(method) = method_of(&entry.key) {
                read.extra.operations.push(Operation {
                    method: method.to_owned(),
                    line: entry.line,
                });
            }
        }
    }
    Ok(read)
}

/// Declares the operations of `paths` under the server base and notes the path items that are references.
fn read_document(root: &Value, read: &mut Read) {
    let base = base_of(root);
    let Some(paths) = root.get("paths") else {
        return;
    };
    for item in paths.entries() {
        let route = format!(
            "{}{}{}",
            base.trim_end_matches('/'),
            if item.key.starts_with('/') { "" } else { "/" },
            item.key
        );
        for member in item.value.entries() {
            if let Some(method) = method_of(&member.key) {
                read.facts
                    .push(http_site(method, &route, Role::Declares, member.line, VIA));
            } else if member.key == "$ref"
                && let Some(spec) = member.value.as_str().and_then(file_of_ref)
            {
                read.extra.path_refs.push(PathRef {
                    spec: spec.to_owned(),
                    line: member.line,
                    route: route.clone(),
                });
            }
        }
    }
}

/// The URL of the first server in OpenAPI 3, or `basePath` in Swagger 2; the key keeps only its path.
fn base_of(root: &Value) -> String {
    root.get("servers")
        .and_then(|servers| servers.items().first())
        .and_then(|server| server.get("url"))
        .and_then(Value::as_str)
        .or_else(|| root.get("basePath").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}

fn method_of(key: &str) -> Option<&'static str> {
    METHODS
        .iter()
        .find(|(lower, _)| lower.eq_ignore_ascii_case(key))
        .map(|(_, upper)| *upper)
}

/// Every `$ref` to another file, at the line of its key.
fn collect_refs(value: &Value, imports: &mut Vec<Import>) {
    for entry in value.entries() {
        if entry.key == "$ref"
            && let Some(spec) = entry.value.as_str().and_then(file_of_ref)
        {
            imports.push(Import::new(spec, ImportKind::Reference, entry.line));
        }
        collect_refs(&entry.value, imports);
    }
    for item in value.items() {
        collect_refs(item, imports);
    }
}

/// The file a `$ref` names: the text before the `#`, nothing for a reference inside the document or to a URL.
fn file_of_ref(reference: &str) -> Option<&str> {
    let file = reference.split('#').next()?.trim();
    (!file.is_empty() && !file.contains("://")).then_some(file)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn route_facts(path: &str, text: &str) -> Vec<(Line, String)> {
        let path = RepoPath::normalize(path).unwrap();
        read(&path, text)
            .map(|read| {
                read.facts
                    .into_iter()
                    .map(|fact| (fact.line, fact.key.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn swagger_two_uses_base_path() {
        let text = "swagger: \"2.0\"\nbasePath: /v2/\npaths:\n  /bookings:\n    get: {}\n";
        assert_eq!(
            route_facts("swagger.yaml", text),
            vec![(5, "http GET /v2/bookings".to_owned())]
        );
    }

    #[test]
    fn json_documents_keep_key_lines() {
        let text = "{\n  \"openapi\": \"3.0.0\",\n  \"paths\": {\n    \"/a\": {\n      \"post\": {}\n    }\n  }\n}\n";
        assert_eq!(
            route_facts("openapi.json", text),
            vec![(5, "http POST /a".to_owned())]
        );
    }

    #[test]
    fn references_inside_the_document_and_to_urls_are_not_imports() {
        let text = "paths:\n  /a:\n    get:\n      x:\n        $ref: '#/components/x'\n      y:\n        $ref: https://example.com/x.yaml\n      z:\n        $ref: ./z.yaml#/Z\n";
        let path = RepoPath::normalize("openapi.yaml").unwrap();
        let imports = read(&path, text).unwrap().imports;
        let specs: Vec<(&str, Line)> = imports
            .iter()
            .map(|import| (import.spec.as_str(), import.line))
            .collect();
        assert_eq!(specs, vec![("./z.yaml", 9)]);
    }

    proptest! {
        #[test]
        fn a_reference_names_no_fragment_and_no_url(reference in "\\PC*") {
            if let Some(file) = file_of_ref(&reference) {
                prop_assert!(!file.is_empty() && !file.contains('#') && !file.contains("://"));
            }
        }

        #[test]
        fn arbitrary_documents_never_panic(text in "\\PC*", json in any::<bool>()) {
            let name = if json { "openapi.json" } else { "openapi.yaml" };
            let path = RepoPath::normalize(name).unwrap();
            drop(read(&path, &text));
        }
    }
}
