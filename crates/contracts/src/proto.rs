//! Protobuf: each `rpc` of a `service` declares `grpc <package>.<Service>/<Method>`, and the `google.api.http` option of an `rpc` declares the `http <METHOD> <path>` of each of its bindings.
//!
//! Imports resolve against the module roots that `buf.yaml` and `buf.work.yaml` name and then against each directory above the importing file, which is how `protoc -I` and `buf` find a file in practice.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{
    Connector, ConnectorError, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    ContractFact, FileFacts, Import, ImportKind, Line, Resolution, UnresolvedReason,
};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use gsttt_core::text::LineIndex;
use gsttt_core::tree::{SyntaxError, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{any_claimed, http_site, provide, site, syntax_failure};
use crate::tree::parse_yaml;

const VIA: &str = "proto";

/// Messages nested deeper than this in an option are not read; real `google.api.http` rules nest twice.
const MAX_NESTING: usize = 8;

/// The connector for protobuf files.
#[derive(Clone, Copy, Debug)]
pub struct Proto;

/// The connector for protobuf files.
pub static PROTO: Erased<Proto> = Erased(Proto);

/// The options of the `proto` connector: it has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ProtoOptions {}

/// The project of the `proto` connector.
#[derive(Clone, Debug)]
pub struct ProtoProject {
    repo: RepoView,
    /// The module roots of `buf.yaml` and `buf.work.yaml` files, sorted.
    roots: Vec<String>,
}

fn claims(path: &RepoPath) -> bool {
    path.extension() == Some("proto")
}

impl Connector for Proto {
    type Options = ProtoOptions;
    type Project = ProtoProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "proto",
            version: 1,
            summary: "gRPC methods declared by protobuf services and the HTTP bindings of their google.api.http options",
            extensions: &["proto"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        any_claimed(repo, claims)
    }

    fn open(
        &self,
        repo: &RepoView,
        _: &ProtoOptions,
        _: &[Pack],
    ) -> Result<Vec<ProtoProject>, ConnectorError> {
        if !any_claimed(repo, claims) {
            return Ok(Vec::new());
        }
        Ok(vec![ProtoProject {
            repo: repo.clone(),
            roots: buf_roots(repo),
        }])
    }
}

impl Project for ProtoProject {
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
                ContractKind::Grpc,
                Role::Declares,
                &["protobuf services"],
                &["schemas generated at build time"],
            ),
            provide(
                ContractKind::Http,
                Role::Declares,
                &["google.api.http options"],
                &["HTTP rules written in a gRPC service config file"],
            ),
        ]
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        match read(file.path.as_str(), file.text) {
            Ok(read) => Parsed::new(FileFacts {
                imports: read.imports,
                contracts: read.facts,
                ..FileFacts::default()
            }),
            Err(error) => Parsed::new(syntax_failure(error)),
        }
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        if import.spec.starts_with("google/protobuf/") {
            return Resolution::Builtin;
        }
        let ancestors = std::iter::successors(Some(from.parent()), |dir| parent_dir(dir));
        for root in self.roots.iter().map(String::as_str).chain(ancestors) {
            let found = RepoPath::join(root, &import.spec)
                .ok()
                .and_then(|target| self.repo.lookup(target.as_str()));
            if let Some(file) = found {
                return Resolution::File { file };
            }
        }
        Resolution::Unresolved(UnresolvedReason::UnknownPackage)
    }
}

/// The directory above `dir`, ending at the repository root, which is the empty string.
fn parent_dir(dir: &str) -> Option<&str> {
    if dir.is_empty() {
        return None;
    }
    Some(dir.rsplit_once('/').map_or("", |(parent, _)| parent))
}

/// The module roots named by the `buf.yaml` and `buf.work.yaml` files of the repository.
///
/// A `buf.yaml` of version 2 lists `modules` with a `path`, a `buf.yaml` of version 1 is the module at its own directory, and a `buf.work.yaml` lists `directories`; all of them count from the directory of the file. A file that cannot be read is skipped and logged, since the ancestors of the importing file still resolve most imports.
fn buf_roots(repo: &RepoView) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for (_, path) in repo.files() {
        let work = match path.file_name() {
            "buf.yaml" | "buf.yml" => false,
            "buf.work.yaml" | "buf.work.yml" => true,
            _ => continue,
        };
        let document = repo
            .read_to_string(path.as_str())
            .map_err(|error| error.to_string())
            .and_then(|text| parse_yaml(&text).map_err(|error| error.message));
        match document {
            Ok(document) => roots.extend(roots_of(path.parent(), &document, work)),
            Err(message) => {
                tracing::warn!(file = path.as_str(), %message, "skipping a buf config that cannot be read");
            }
        }
    }
    roots.into_iter().collect()
}

fn roots_of(dir: &str, document: &Value, work: bool) -> Vec<String> {
    let relative: Vec<&str> = if work {
        document
            .get("directories")
            .map(Value::items)
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .collect()
    } else {
        document
            .get("modules")
            .map(Value::items)
            .unwrap_or_default()
            .iter()
            .map(|module| module.get("path").and_then(Value::as_str).unwrap_or("."))
            .collect()
    };
    if relative.is_empty() && !work {
        // Version 1 has no `modules`: the module is the directory itself.
        return vec![dir.to_owned()];
    }
    relative
        .into_iter()
        .filter_map(|path| {
            if matches!(path, "" | ".") {
                Some(dir.to_owned())
            } else {
                RepoPath::join(dir, path)
                    .ok()
                    .map(|joined| joined.as_str().to_owned())
            }
        })
        .collect()
}

/// What one proto file says.
#[derive(Debug, Default)]
struct Read {
    facts: Vec<ContractFact>,
    imports: Vec<Import>,
}

fn read(name: &str, text: &str) -> Result<Read, SyntaxError> {
    let descriptor = protox_parse::parse(name, text).map_err(|error| SyntaxError {
        line: error
            .span()
            .map_or(1, |span| LineIndex::new(text).line_at(span.start)),
        message: error.to_string(),
    })?;
    // The descriptor numbers its locations by path (`[6, 0, 2, 1]` is the second rpc of the first service); the first line of each is what the sites need.
    let mut lines_at: BTreeMap<&[i32], Line> = BTreeMap::new();
    if let Some(info) = &descriptor.source_code_info {
        for location in &info.location {
            if let Some(first) = location
                .span
                .first()
                .and_then(|line| u32::try_from(*line).ok())
            {
                lines_at
                    .entry(location.path.as_slice())
                    .or_insert_with(|| first.saturating_add(1));
            }
        }
    }
    let line_at = |path: &[i32]| lines_at.get(path).copied();

    let mut read = Read::default();
    for (index, dependency) in descriptor.dependency.iter().enumerate() {
        let line = i32::try_from(index)
            .ok()
            .and_then(|index| line_at(&[3, index]))
            .unwrap_or(1);
        read.imports
            .push(Import::new(dependency.as_str(), ImportKind::Static, line));
    }
    let package = descriptor.package.as_deref().unwrap_or_default();
    for (s, service) in descriptor.service.iter().enumerate() {
        let service_name = service.name.as_deref().unwrap_or_default();
        for (m, method) in service.method.iter().enumerate() {
            let (Ok(s), Ok(m)) = (i32::try_from(s), i32::try_from(m)) else {
                continue;
            };
            let method_name = method.name.as_deref().unwrap_or_default();
            let key = if package.is_empty() {
                format!("{service_name}/{method_name}")
            } else {
                format!("{package}.{service_name}/{method_name}")
            };
            let rpc_line = line_at(&[6, s, 2, m]).unwrap_or(1);
            read.facts.push(site(
                ContractKind::Grpc,
                &key,
                Role::Declares,
                rpc_line,
                VIA,
            ));
            let Some(options) = &method.options else {
                continue;
            };
            for (o, option) in options.uninterpreted_option.iter().enumerate() {
                let Some((extension, field)) = option.name.split_first() else {
                    continue;
                };
                if !extension.is_extension
                    || extension.name_part.trim_start_matches('.') != "google.api.http"
                {
                    continue;
                }
                let field = match field {
                    [] => None,
                    [one] => Some(one.name_part.as_str()),
                    _ => continue,
                };
                // The option statement itself when the descriptor locates it, the rpc otherwise.
                let line = i32::try_from(o)
                    .ok()
                    .and_then(|o| line_at(&[6, s, 2, m, 4, 999, o]))
                    .unwrap_or(rpc_line);
                let found = option_bindings(
                    field,
                    option.aggregate_value.as_deref(),
                    option.string_value.as_deref(),
                );
                for binding in found {
                    read.facts.push(http_site(
                        &binding.method,
                        &variables_as_parameters(&binding.path),
                        Role::Declares,
                        line,
                        VIA,
                    ));
                }
            }
        }
    }
    Ok(read)
}

/// One HTTP binding of a `google.api.http` rule.
#[derive(Debug, PartialEq, Eq)]
struct Binding {
    method: String,
    path: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Text(String),
    Open,
    Close,
    Colon,
}

/// A value in the text format of a message: a string or a nested message.
#[derive(Debug)]
enum Field {
    Text(String),
    Message(Vec<(String, Field)>),
}

/// The bindings one `option (google.api.http)` statement sets. The parser keeps `= { get: "/a" }` as the text of the rule, and `.get = "/a"`, which sets one field of the rule, as the name of the field with its value: the bytes of a string or, for a message field, its text.
fn option_bindings(
    field: Option<&str>,
    message: Option<&str>,
    text: Option<&[u8]>,
) -> Vec<Binding> {
    let fields = match (field, message, text) {
        (None, Some(rule), _) => return bindings(rule),
        (Some(name), Some(rule), _) => {
            vec![(name.to_owned(), Field::Message(fields_of(rule)))]
        }
        (Some(name), None, Some(path)) => vec![(
            name.to_owned(),
            Field::Text(String::from_utf8_lossy(path).into_owned()),
        )],
        _ => return Vec::new(),
    };
    let mut found = Vec::new();
    collect_bindings(&fields, &mut found);
    found
}

/// The bindings of a rule in the text format the parser keeps it in: `get : "/v1/a" additional_bindings { post : "/v1/b" }`. Only the shapes of the `HttpRule` message are read; anything else in it is skipped.
fn bindings(rule: &str) -> Vec<Binding> {
    let mut found = Vec::new();
    collect_bindings(&fields_of(rule), &mut found);
    found
}

fn fields_of(rule: &str) -> Vec<(String, Field)> {
    let tokens = tokenize(rule);
    let mut at = 0;
    read_fields(&tokens, &mut at, 0)
}

/// A path template with each variable, `{id}` or `{name=projects/*}`, written as the `{}` of a parameter in a key. The shared normaliser reads a segment that holds a `{…}` as a parameter, but it splits the path at every `/`, so it cannot see a variable that spans several segments.
fn variables_as_parameters(template: &str) -> String {
    let mut path = String::with_capacity(template.len());
    let mut depth = 0_usize;
    for c in template.chars() {
        match c {
            '{' => {
                if depth == 0 {
                    path.push_str("{}");
                }
                depth = depth.saturating_add(1);
            }
            '}' if depth > 0 => depth = depth.saturating_sub(1),
            _ if depth == 0 => path.push(c),
            _ => {}
        }
    }
    path
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '<' => tokens.push(Token::Open),
            '}' | '>' => tokens.push(Token::Close),
            ':' => tokens.push(Token::Colon),
            '"' | '\'' => {
                let mut value = String::new();
                while let Some(next) = chars.next() {
                    if next == c {
                        break;
                    }
                    if next == '\\' {
                        // The escaped character stands for itself: paths hold no other escape that matters.
                        if let Some(escaped) = chars.next() {
                            value.push(escaped);
                        }
                    } else {
                        value.push(next);
                    }
                }
                tokens.push(Token::Text(value));
            }
            c if c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '+') => {
                let mut word = String::from(c);
                while let Some(next) = chars
                    .next_if(|next| next.is_alphanumeric() || matches!(next, '_' | '.' | '-' | '+'))
                {
                    word.push(next);
                }
                tokens.push(Token::Word(word));
            }
            _ => {}
        }
    }
    tokens
}

/// The fields of one message, up to its closing brace.
fn read_fields(tokens: &[Token], at: &mut usize, depth: usize) -> Vec<(String, Field)> {
    let mut fields = Vec::new();
    while let Some(token) = tokens.get(*at) {
        *at += 1;
        match token {
            Token::Close => break,
            Token::Word(key) => {
                if tokens.get(*at) == Some(&Token::Colon) {
                    *at += 1;
                }
                match tokens.get(*at) {
                    Some(Token::Open) => {
                        *at += 1;
                        if depth < MAX_NESTING {
                            fields.push((
                                key.clone(),
                                Field::Message(read_fields(tokens, at, depth + 1)),
                            ));
                        } else {
                            skip_message(tokens, at);
                        }
                    }
                    Some(Token::Text(_)) => {
                        // Neighbouring string literals are one string.
                        let mut value = String::new();
                        while let Some(Token::Text(part)) = tokens.get(*at) {
                            value.push_str(part);
                            *at += 1;
                        }
                        fields.push((key.clone(), Field::Text(value)));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fields
}

/// Moves past the message whose opening brace was just read.
fn skip_message(tokens: &[Token], at: &mut usize) {
    let mut level = 1_usize;
    while let Some(token) = tokens.get(*at) {
        *at += 1;
        match token {
            Token::Open => level += 1,
            Token::Close => {
                level = level.saturating_sub(1);
                if level == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

fn collect_bindings(fields: &[(String, Field)], found: &mut Vec<Binding>) {
    for (key, field) in fields {
        match (key.as_str(), field) {
            ("get" | "put" | "post" | "delete" | "patch", Field::Text(path)) => {
                found.push(Binding {
                    method: key.to_uppercase(),
                    path: path.clone(),
                });
            }
            ("custom", Field::Message(custom)) => {
                if let (Some(kind), Some(path)) = (text_of(custom, "kind"), text_of(custom, "path"))
                {
                    found.push(Binding {
                        method: kind.to_uppercase(),
                        path: path.to_owned(),
                    });
                }
            }
            ("additional_bindings", Field::Message(inner)) => collect_bindings(inner, found),
            _ => {}
        }
    }
}

fn text_of<'a>(fields: &'a [(String, Field)], key: &str) -> Option<&'a str> {
    fields.iter().find_map(|(name, field)| match field {
        Field::Text(text) if name == key => Some(text.as_str()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use gsttt_core::contract::ContractKey;
    use proptest::prelude::*;

    use super::*;

    fn binding(method: &str, path: &str) -> Binding {
        Binding {
            method: method.to_owned(),
            path: path.to_owned(),
        }
    }

    #[test]
    fn rules_give_their_bindings_and_additional_ones() {
        let rule = r#"get : "/v1/a/{id}" additional_bindings { post : "/v1/b" body : "*" additional_bindings { delete : "/v1/c" } } custom { kind : "HEAD" path : "/h" }"#;
        assert_eq!(
            bindings(rule),
            vec![
                binding("GET", "/v1/a/{id}"),
                binding("POST", "/v1/b"),
                binding("DELETE", "/v1/c"),
                binding("HEAD", "/h"),
            ]
        );
    }

    #[test]
    fn message_delimiters_and_split_strings_read_like_braces() {
        let rule = r#"get : "/v1/" "x" additional_bindings < put : "/v2" >"#;
        assert_eq!(
            bindings(rule),
            vec![binding("GET", "/v1/x"), binding("PUT", "/v2")]
        );
    }

    /// The HTTP keys of the one rpc that `option` is written in.
    fn http_keys(option: &str) -> Vec<ContractKey> {
        let text = format!(
            "syntax = \"proto3\";\npackage a.v1;\nservice S {{ rpc M(E) returns (E) {{ {option} }} }}\nmessage E {{}}\n"
        );
        read("a.proto", &text)
            .unwrap()
            .facts
            .into_iter()
            .map(|fact| fact.key)
            .filter(|key| *key.kind() == ContractKind::Http)
            .collect()
    }

    #[test]
    fn a_variable_and_the_verb_after_it_are_one_parameter() {
        // A client writes the same route as `/v1/things/${id}:cancel`.
        assert_eq!(
            http_keys(r#"option (google.api.http) = { post: "/v1/things/{id}:cancel" };"#),
            vec![ContractKey::http("POST", "/v1/things/${id}:cancel")]
        );
    }

    #[test]
    fn a_variable_that_spans_segments_is_one_parameter() {
        assert_eq!(
            http_keys(
                r#"option (google.api.http) = { get: "/v1/{parent=projects/*/locations/*}/things" };"#
            ),
            vec![ContractKey::http("GET", "/v1/${parent}/things")]
        );
    }

    #[test]
    fn a_binding_set_one_field_at_a_time_is_read() {
        assert_eq!(
            http_keys(
                r#"option (google.api.http).get = "/v1/things/{id}"; option (google.api.http).body = "*";"#
            ),
            vec![ContractKey::http("GET", "/v1/things/${id}")]
        );
        assert_eq!(
            http_keys(r#"option (google.api.http).additional_bindings = { post: "/v1/b" };"#),
            vec![ContractKey::http("POST", "/v1/b")]
        );
    }

    #[test]
    fn variables_become_empty_braces() {
        for (template, expected) in [
            ("/v1/things/{id}", "/v1/things/{}"),
            ("/v1/things/{id}:cancel", "/v1/things/{}:cancel"),
            ("/v1/{name=projects/*/locations/*}/things", "/v1/{}/things"),
            ("/v1/{thing.id=things/*}:get", "/v1/{}:get"),
            ("/v1/things:batchGet", "/v1/things:batchGet"),
            ("/v1/{id", "/v1/{}"),
            ("/v1/id}", "/v1/id}"),
        ] {
            assert_eq!(variables_as_parameters(template), expected, "{template}");
        }
    }

    #[test]
    fn directories_above_a_file_are_tried_nearest_first() {
        let dirs: Vec<&str> = std::iter::successors(Some("a/b/c"), |dir| parent_dir(dir)).collect();
        assert_eq!(dirs, vec!["a/b/c", "a/b", "a", ""]);
    }

    proptest! {
        #[test]
        fn arbitrary_rules_never_panic(rule in "\\PC*") {
            drop(bindings(&rule));
        }

        #[test]
        fn a_variable_is_never_more_than_an_empty_pair_of_braces(template in "[a-z/{}=*:_.-]{0,30}") {
            let path = variables_as_parameters(&template);
            prop_assert!(
                path.match_indices('{')
                    .all(|(at, _)| path.get(at..).is_some_and(|rest| rest.starts_with("{}"))),
                "{path}"
            );
            prop_assert_eq!(variables_as_parameters(&path), path);
        }

        #[test]
        fn a_simple_binding_is_read_back(
            method in prop::sample::select(vec!["get", "put", "post", "delete", "patch"]),
            path in "/[a-z0-9/{}_-]{0,20}",
        ) {
            let rule = format!("{method} : \"{path}\"");
            prop_assert_eq!(bindings(&rule), vec![binding(&method.to_uppercase(), &path)]);
        }

        #[test]
        fn arbitrary_text_is_a_failure_or_keeps_lines_in_range(text in "\\PC*") {
            let lines = u32::try_from(text.lines().count().max(1)).unwrap_or(u32::MAX);
            if let Ok(read) = read("x.proto", &text) {
                for fact in read.facts {
                    prop_assert!(fact.line >= 1 && fact.line <= lines);
                }
            }
        }
    }
}
