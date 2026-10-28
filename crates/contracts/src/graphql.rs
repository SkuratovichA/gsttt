//! GraphQL schemas and operations, in `.graphql`, `.graphqls` and `.gql` files and in the documents other
//! connectors find embedded in code.
//!
//! A schema declares `graphql Query.bookings` for each field of a root type, an operation calls the root fields it
//! selects, and a `#import "path"` comment is an import, the convention of the graphql-tag loaders. The
//! `# import <names> from "<file>"` form of graphql-import is not read, and neither is a root type renamed in
//! another file than the one that defines it; both are blind spots of the connector.
//!
//! The connector always opens a project, even where no file is claimed, because code in any language can embed
//! GraphQL documents.

use std::collections::{BTreeMap, BTreeSet};

use apollo_parser::Parser;
use apollo_parser::cst::{self, Definition, Selection};
use gsttt_core::connector::{
    Connector, ConnectorError, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    ContractFact, Embedded, ErrorCount, FileFacts, Import, ImportKind, Line, Resolution,
};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use gsttt_core::text::LineIndex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{byte_offset, line_number, provide, resolve_relative, site};

const VIA: &str = "graphql";

/// Said by both roles: an import written this way is no edge of the graph, so the files it joins stay apart.
const UNREAD_IMPORTS: &str = "imports written `# import <names> from \"<file>\"`";

/// The connector for GraphQL.
#[derive(Clone, Copy, Debug)]
pub struct Graphql;

/// The connector for GraphQL.
pub static GRAPHQL: Erased<Graphql> = Erased(Graphql);

/// The options of the `graphql` connector: it has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GraphqlOptions {}

/// The project of the `graphql` connector; it keeps the file list to resolve `#import` comments against.
#[derive(Clone, Debug)]
pub struct GraphqlProject {
    repo: RepoView,
}

fn claims(path: &RepoPath) -> bool {
    matches!(path.extension(), Some("graphql" | "graphqls" | "gql"))
}

impl Connector for Graphql {
    type Options = GraphqlOptions;
    type Project = GraphqlProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "graphql",
            version: 1,
            summary: "GraphQL schema fields and the root fields operations call, in files and embedded in code",
            extensions: &["graphql", "graphqls", "gql"],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        // Code in any language can embed a GraphQL document, so there is always something it may have to read.
        true
    }

    fn open(
        &self,
        repo: &RepoView,
        _: &GraphqlOptions,
        _: &[Pack],
    ) -> Result<Vec<GraphqlProject>, ConnectorError> {
        Ok(vec![GraphqlProject { repo: repo.clone() }])
    }
}

impl Project for GraphqlProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        claims(path)
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        vec![
            provide(
                ContractKind::Graphql,
                Role::Declares,
                &["schema definitions and their extensions"],
                &[
                    "schemas generated at build time",
                    "root types renamed by a `schema` definition in another file",
                    UNREAD_IMPORTS,
                ],
            ),
            provide(
                ContractKind::Graphql,
                Role::Calls,
                &["operation documents", "documents embedded in code"],
                &["GraphQL fragments spread from other files", UNREAD_IMPORTS],
            ),
        ]
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        let mut errors = ErrorCount::default();
        let mut contracts = Vec::new();
        // A file of only comments is a valid, empty document here, though the grammar wants a definition.
        if !is_blank(file.text) {
            let lines = LineIndex::new(file.text);
            let tree = Parser::new(file.text).parse();
            for error in tree.errors() {
                errors.note(lines.line(byte_offset(error.index())));
            }
            contracts = read_document(&tree.document(), &lines);
        }
        Parsed::new(FileFacts {
            status: errors.status(),
            imports: import_comments(file.text),
            contracts,
            ..FileFacts::default()
        })
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        resolve_relative(&self.repo, from, &import.spec)
    }

    fn embeds(&self) -> &[&str] {
        &["graphql"]
    }

    fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact> {
        if is_blank(&document.text) {
            return Vec::new();
        }
        let lines = LineIndex::new(&document.text);
        let tree = Parser::new(&document.text).parse();
        // Lines count from the document's first line; the engine places them in the host file.
        read_document(&tree.document(), &lines)
    }
}

/// Whether the text holds nothing but blank lines and `#` comments.
fn is_blank(text: &str) -> bool {
    text.lines().all(|line| {
        let line = line.trim_start();
        line.is_empty() || line.starts_with('#')
    })
}

/// The `#import "path"` comments of a file, each an import relative to the file.
fn import_comments(text: &str) -> Vec<Import> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let rest = line
                .trim_start()
                .strip_prefix('#')?
                .trim_start()
                .strip_prefix("import")?
                .trim_start();
            let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            let (spec, _) = rest.get(1..)?.split_once(quote)?;
            (!spec.is_empty()).then(|| Import::new(spec, ImportKind::Include, line_number(index)))
        })
        .collect()
}

/// The three kinds of root operation, spelled the way keys spell them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Query,
    Mutation,
    Subscription,
}

impl Root {
    const ALL: [Self; 3] = [Self::Query, Self::Mutation, Self::Subscription];

    fn name(self) -> &'static str {
        match self {
            Self::Query => "Query",
            Self::Mutation => "Mutation",
            Self::Subscription => "Subscription",
        }
    }

    fn of(operation: &cst::OperationType) -> Self {
        if operation.mutation_token().is_some() {
            Self::Mutation
        } else if operation.subscription_token().is_some() {
            Self::Subscription
        } else {
            Self::Query
        }
    }
}

/// The sites of a document: the root fields its schema declares and the root fields its operations call.
fn read_document(document: &cst::Document, lines: &LineIndex) -> Vec<ContractFact> {
    let definitions: Vec<Definition> = document.definitions().collect();
    // A type looks itself up among the roots and a spread looks up its fragment, so a document without a type or
    // without a fragment, the usual embedded one, builds neither map.
    let roots = if definitions.iter().any(|definition| {
        matches!(
            definition,
            Definition::ObjectTypeDefinition(_) | Definition::ObjectTypeExtension(_)
        )
    }) {
        root_types(&definitions)
    } else {
        BTreeMap::new()
    };
    let fragments = if definitions
        .iter()
        .any(|definition| matches!(definition, Definition::FragmentDefinition(_)))
    {
        fragments_of(&definitions)
    } else {
        BTreeMap::new()
    };
    let mut facts = Vec::new();
    for definition in &definitions {
        match definition {
            Definition::ObjectTypeDefinition(object) => {
                declare(
                    object.name().as_ref(),
                    object.fields_definition().as_ref(),
                    &roots,
                    lines,
                    &mut facts,
                );
            }
            Definition::ObjectTypeExtension(object) => {
                declare(
                    object.name().as_ref(),
                    object.fields_definition().as_ref(),
                    &roots,
                    lines,
                    &mut facts,
                );
            }
            Definition::OperationDefinition(operation) => {
                call_roots(operation, &fragments, lines, &mut facts);
            }
            _ => {}
        }
    }
    // Source order, the same everywhere; one site per line and key even when a fragment is reached twice.
    facts.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.key.cmp(&b.key)));
    facts.dedup_by(|a, b| a.line == b.line && a.key == b.key && a.role == b.role);
    facts
}

/// The type that plays each root: `Query`, `Mutation` and `Subscription` unless a `schema` definition or extension
/// names another, which then replaces the default for that operation.
fn root_types(definitions: &[Definition]) -> BTreeMap<String, Root> {
    let mut named: BTreeMap<Root, String> = Root::ALL
        .iter()
        .map(|root| (*root, root.name().to_owned()))
        .collect();
    for definition in definitions {
        let schema_roots = match definition {
            Definition::SchemaDefinition(schema) => schema.root_operation_type_definitions(),
            Definition::SchemaExtension(schema) => schema.root_operation_type_definitions(),
            _ => continue,
        };
        for root in schema_roots {
            let (Some(operation), Some(type_name)) = (
                root.operation_type(),
                root.named_type().and_then(|named_type| named_type.name()),
            ) else {
                continue;
            };
            named.insert(Root::of(&operation), name_of(&type_name));
        }
    }
    named
        .into_iter()
        .map(|(root, type_name)| (type_name, root))
        .collect()
}

fn fragments_of(definitions: &[Definition]) -> BTreeMap<String, cst::SelectionSet> {
    definitions
        .iter()
        .filter_map(|definition| {
            let Definition::FragmentDefinition(fragment) = definition else {
                return None;
            };
            let name = fragment.fragment_name()?.name()?;
            Some((name_of(&name), fragment.selection_set()?))
        })
        .collect()
}

/// Declares the fields of an object type, or of its extension, when it is a root type.
fn declare(
    type_name: Option<&cst::Name>,
    fields: Option<&cst::FieldsDefinition>,
    roots: &BTreeMap<String, Root>,
    lines: &LineIndex,
    facts: &mut Vec<ContractFact>,
) {
    let Some(root) = type_name.and_then(|name| roots.get(&name_of(name)).copied()) else {
        return;
    };
    for field in fields
        .into_iter()
        .flat_map(cst::FieldsDefinition::field_definitions)
    {
        let Some(name) = field.name() else { continue };
        let key = format!("{}.{}", root.name(), name_of(&name));
        facts.push(site(
            ContractKind::Graphql,
            &key,
            Role::Declares,
            name_line(&name, lines),
            VIA,
        ));
    }
}

/// Calls the root fields an operation selects.
fn call_roots(
    operation: &cst::OperationDefinition,
    fragments: &BTreeMap<String, cst::SelectionSet>,
    lines: &LineIndex,
    facts: &mut Vec<ContractFact>,
) {
    let Some(selections) = operation.selection_set() else {
        return;
    };
    // An anonymous operation, `{ bookings { id } }`, is a query.
    let root = operation
        .operation_type()
        .as_ref()
        .map_or(Root::Query, Root::of);
    let mut spread = BTreeSet::new();
    walk(&selections, root, fragments, lines, &mut spread, facts);
}

/// Calls the fields written at the root of a selection set. A fragment spread or an inline fragment at the root
/// holds more root fields; `spread` keeps a cycle of fragments from running forever.
fn walk(
    selections: &cst::SelectionSet,
    root: Root,
    fragments: &BTreeMap<String, cst::SelectionSet>,
    lines: &LineIndex,
    spread: &mut BTreeSet<String>,
    facts: &mut Vec<ContractFact>,
) {
    for selection in selections.selections() {
        match selection {
            Selection::Field(field) => {
                let Some(name) = field.name() else { continue };
                let text = name_of(&name);
                // `__typename` and the introspection fields belong to no schema.
                if text.starts_with("__") {
                    continue;
                }
                facts.push(site(
                    ContractKind::Graphql,
                    &format!("{}.{text}", root.name()),
                    Role::Calls,
                    name_line(&name, lines),
                    VIA,
                ));
            }
            Selection::FragmentSpread(fragment) => {
                let Some(name) = fragment.fragment_name().and_then(|name| name.name()) else {
                    continue;
                };
                let name = name_of(&name);
                if let Some(body) = fragments.get(&name)
                    && spread.insert(name)
                {
                    walk(body, root, fragments, lines, spread, facts);
                }
            }
            Selection::InlineFragment(fragment) => {
                if let Some(inner) = fragment.selection_set() {
                    walk(&inner, root, fragments, lines, spread, facts);
                }
            }
        }
    }
}

fn name_of(name: &cst::Name) -> String {
    String::from(name.text())
}

fn name_line(name: &cst::Name, lines: &LineIndex) -> Line {
    name.ident_token()
        .map_or(1, |token| lines.line(u32::from(token.text_range().start())))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn read(text: &str) -> Vec<(Line, String, Role)> {
        let tree = Parser::new(text).parse();
        read_document(&tree.document(), &LineIndex::new(text))
            .into_iter()
            .map(|fact| (fact.line, fact.key.to_string(), fact.role))
            .collect()
    }

    #[test]
    fn a_cycle_of_fragments_ends() {
        let text = "fragment A on Query { me { id } ...B }\nfragment B on Query { ...A }\nquery { ...A }\n";
        assert_eq!(
            read(text),
            vec![(1, "graphql Query.me".to_owned(), Role::Calls)]
        );
    }

    #[test]
    fn introspection_fields_call_nothing() {
        assert_eq!(
            read("query { __typename __schema { types { name } } }"),
            Vec::new()
        );
    }

    #[test]
    fn import_comments_need_a_quoted_path() {
        let imports = import_comments(
            "#import \"./a.graphql\"\n# import './b.graphql'\n#important \"c\"\n#import d\n",
        );
        let specs: Vec<(&str, Line)> = imports
            .iter()
            .map(|import| (import.spec.as_str(), import.line))
            .collect();
        assert_eq!(specs, vec![("./a.graphql", 1), ("./b.graphql", 2)]);
    }

    proptest! {
        #[test]
        fn arbitrary_text_never_panics_and_keeps_lines_in_range(text in "\\PC*") {
            let lines = u32::try_from(text.lines().count().max(1)).unwrap_or(u32::MAX);
            for (line, _, _) in read(&text) {
                prop_assert!(line >= 1 && line <= lines);
            }
            for import in import_comments(&text) {
                prop_assert!(import.line >= 1 && import.line <= lines);
            }
        }

        #[test]
        fn operations_call_each_root_field_at_its_line(
            names in prop::collection::vec("[a-z][A-Za-z0-9]{0,8}", 1..8)
        ) {
            let mut body = String::new();
            for name in &names {
                body.push_str("  ");
                body.push_str(name);
                body.push('\n');
            }
            let text = format!("query {{\n{body}}}\n");
            let expected: Vec<(Line, String)> = names
                .iter()
                .enumerate()
                .map(|(index, name)| (line_number(index + 1), format!("graphql Query.{name}")))
                .collect();
            let mut expected = expected;
            expected.sort();
            expected.dedup();
            let got: Vec<(Line, String)> = read(&text).into_iter().map(|(line, key, _)| (line, key)).collect();
            prop_assert_eq!(got, expected);
        }
    }
}
