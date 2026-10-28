//! Running a fixture tree through the whole pipeline, and reading the output back as short lines.
//!
//! The snapshot of a test holds everything the connector found. The readers below give the few facts a scenario speaks about, so that a test says what it proves even when a snapshot is accepted in a hurry.

use std::error::Error;

use gsttt_config::builtin_packs;
use gsttt_core::RepoView;
use gsttt_core::engine::build::build_index;
use gsttt_core::engine::pipeline::{self, Module, Output};
use gsttt_core::fact::{FileTags, ImportFlags};
use gsttt_core::index::{Freshness, Index, IndexInfo};
use gsttt_core::query::{ImpactRequest, impact};
use serde_json::Value;

/// The size above which a file is left unread; the fixtures are far below it.
const MAX_FILE_BYTES: u64 = 1_000_000;

/// What a reader returns when the output holds nothing of the kind, typed so that a test can compare against it.
pub(crate) const NOTHING: [&str; 0] = [];

/// Runs the tree `tests/fixtures/<fixture>` through the pipeline with the TypeScript connector and its built-in packs.
fn pipeline_of(fixture: &str) -> Result<(RepoView, Output), Box<dyn Error>> {
    let root = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    let repo = RepoView::scan(root)?;
    let modules = [Module {
        connector: &gsttt_lang_ts::TYPESCRIPT,
        options: Value::Null,
        packs: builtin_packs(gsttt_lang_ts::PACKS)?,
    }];
    let output = pipeline::run(&repo, &modules, MAX_FILE_BYTES)?;
    Ok((repo, output))
}

/// Indexes the tree `tests/fixtures/<fixture>` with the TypeScript connector and its built-in packs, and returns the output in the JSON form that is snapshotted, with the keys of every object in byte order.
pub(crate) fn run(fixture: &str) -> Result<Value, Box<dyn Error>> {
    let (repo, output) = pipeline_of(fixture)?;
    // Sorted here, once, so that no snapshot depends on whether serde_json keeps insertion order (its `preserve_order` feature, which another crate of the workspace may switch on).
    let mut json = output.to_json(&repo);
    json.sort_all_objects();
    Ok(json)
}

/// The index the engine builds from the output for `fixture`. Unlike the output, its graph has applied the rule that a linked fact replaces the own fact it names, so it is what a query would answer from.
fn index_of(fixture: &str) -> Result<Index, Box<dyn Error>> {
    let (repo, output) = pipeline_of(fixture)?;
    Ok(build_index(
        &repo,
        &output,
        &["ts"],
        &[],
        IndexInfo::default(),
    ))
}

/// Every site of the graph for `fixture`, as `<role> <key> @<line> in <path>`, in byte order.
pub(crate) fn sites(fixture: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let index = index_of(fixture)?;
    let graph = &index.graph;
    let mut sites: Vec<String> = graph
        .contracts()
        .flat_map(|(id, key)| {
            graph.sites(id).iter().map(move |site| {
                let path = graph.path(site.file).map_or("?", |path| path.as_str());
                format!("{} {key} @{} in {path}", site.role, site.line)
            })
        })
        .collect();
    sites.sort();
    Ok(sites)
}

/// What a change to the file at `path` affects in `fixture`, as `<path> <distance> <reason>` in the order of the answer, without the tests.
pub(crate) fn affected_by(fixture: &str, path: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let request = ImpactRequest {
        files: vec![path.to_owned()],
        ..ImpactRequest::default()
    };
    let answer = impact(&index_of(fixture)?, Freshness::Fresh, &request);
    Ok(answer
        .affected
        .iter()
        .map(|entry| format!("{} {} {}", entry.path, entry.distance, entry.reason))
        .collect())
}

/// The roots of the projects, in the order of the output.
pub(crate) fn roots(json: &Value) -> Vec<&str> {
    items(json, "projects")
        .filter_map(|project| text(project, "root"))
        .collect()
}

/// The project the file at `path` belongs to, as the output names it.
pub(crate) fn project_of<'a>(json: &'a Value, path: &str) -> Option<&'a str> {
    file(json, path).and_then(|file| text(file, "project"))
}

/// What the project at `root` provides: `<kind> <role> via <packs>`.
pub(crate) fn provides(json: &Value, root: &str) -> Vec<String> {
    items(project(json, root), "provides")
        .map(|provided| {
            let packs: Vec<&str> = items(provided, "via").filter_map(Value::as_str).collect();
            format!(
                "{} {} via {}",
                text(provided, "kind").unwrap_or("?"),
                text(provided, "role").unwrap_or("?"),
                packs.join(",")
            )
        })
        .collect()
}

/// The blind spots the project at `root` names, in words.
pub(crate) fn blind_spots(json: &Value, root: &str) -> Vec<String> {
    items(project(json, root), "provides")
        .flat_map(|provided| items(provided, "blind_spots"))
        .filter_map(|spot| spot.as_str().map(str::to_owned))
        .collect()
}

/// The parse status of the file at `path`: `ok`, `recovered <errors> @<first line>` or `failed <kind> @<line>`.
pub(crate) fn status(json: &Value, path: &str) -> String {
    let Some(status) = file(json, path).and_then(|file| file.get("status")) else {
        return "no such file".to_owned();
    };
    match text(status, "status") {
        Some("ok") => "ok".to_owned(),
        Some("recovered") => format!(
            "recovered {} @{}",
            number(status, "errors"),
            number(status, "first_error_line")
        ),
        Some("failed") => format!(
            "failed {} @{}",
            text(status, "failure").unwrap_or("?"),
            number(status, "line")
        ),
        _ => status.to_string(),
    }
}

/// The tags of the file at `path`.
pub(crate) fn tags(json: &Value, path: &str) -> FileTags {
    file(json, path)
        .and_then(|file| file.get("tags"))
        .and_then(|tags| serde_json::from_value(tags.clone()).ok())
        .unwrap_or_default()
}

/// The imports of the file at `path`, in file order: `<specifier> <kind> @<line> [type-only] [<names>] -> <target>`.
pub(crate) fn imports(json: &Value, path: &str) -> Vec<String> {
    file(json, path)
        .into_iter()
        .flat_map(|file| items(file, "imports"))
        .map(|import| {
            let flags: ImportFlags = import
                .get("flags")
                .and_then(|flags| serde_json::from_value(flags.clone()).ok())
                .unwrap_or_default();
            let names: Vec<&str> = items(import, "names").filter_map(Value::as_str).collect();
            let mut parts = vec![
                text(import, "spec").unwrap_or("?").to_owned(),
                text(import, "kind").unwrap_or("?").to_owned(),
                format!("@{}", number(import, "line")),
            ];
            if flags.contains(ImportFlags::TYPE_ONLY) {
                parts.push("type-only".to_owned());
            }
            if !names.is_empty() {
                parts.push(format!("[{}]", names.join(",")));
            }
            parts.push("->".to_owned());
            parts.push(target(import.get("to")));
            parts.join(" ")
        })
        .collect()
}

/// The names the file at `path` exports itself, `None` when the output has no such file or says they are unknown.
pub(crate) fn exports<'a>(json: &'a Value, path: &str) -> Option<Vec<&'a str>> {
    let exports = file(json, path)?.get("exports")?.as_array()?;
    Some(exports.iter().filter_map(Value::as_str).collect())
}

/// What the named re-exports of the file at `path` export each name as, in file order: `<specifier> <name> as <exported>, …`.
pub(crate) fn exported_as(json: &Value, path: &str) -> Vec<String> {
    file(json, path)
        .into_iter()
        .flat_map(|file| items(file, "imports"))
        .filter(|import| items(import, "exported").next().is_some())
        .map(|import| {
            let names = items(import, "names").filter_map(Value::as_str);
            let exported = items(import, "exported").filter_map(Value::as_str);
            let pairs: Vec<String> = names
                .zip(exported)
                .map(|(name, exported)| format!("{name} as {exported}"))
                .collect();
            format!(
                "{} {}",
                text(import, "spec").unwrap_or("?"),
                pairs.join(", ")
            )
        })
        .collect()
}

/// The contract facts the file at `path` carries: `<role> <key> @<line>`.
pub(crate) fn contracts(json: &Value, path: &str) -> Vec<String> {
    file(json, path)
        .into_iter()
        .flat_map(|file| items(file, "contracts"))
        .map(fact)
        .collect()
}

/// The facts that `link` added, each with its file: `<role> <key> @<line> in <path>`.
pub(crate) fn linked(json: &Value) -> Vec<String> {
    items(json, "linked")
        .map(|linked| {
            format!(
                "{} in {}",
                fact(linked),
                text(linked, "file").unwrap_or("?")
            )
        })
        .collect()
}

/// The sites of the file at `path` that were not read: `@<line> <reason> <detail>`.
pub(crate) fn unread(json: &Value, path: &str) -> Vec<String> {
    file(json, path)
        .into_iter()
        .flat_map(|file| items(file, "unresolved"))
        .map(|site| {
            let reason = site
                .get("reason")
                .and_then(|reason| text(reason, "reason"))
                .unwrap_or("?");
            format!(
                "@{} {reason} {}",
                number(site, "line"),
                text(site, "detail").unwrap_or("")
            )
        })
        .collect()
}

/// The documents embedded in the file at `path`: `<language> @<line> <text>`.
pub(crate) fn embedded(json: &Value, path: &str) -> Vec<String> {
    file(json, path)
        .into_iter()
        .flat_map(|file| items(file, "embedded"))
        .map(|document| {
            format!(
                "{} @{} {}",
                text(document, "lang").unwrap_or("?"),
                number(document, "line"),
                text(document, "text").unwrap_or("")
            )
        })
        .collect()
}

fn fact(fact: &Value) -> String {
    format!(
        "{} {} @{}",
        text(fact, "role").unwrap_or("?"),
        text(fact, "key").unwrap_or("?"),
        number(fact, "line")
    )
}

fn target(to: Option<&Value>) -> String {
    let Some(to) = to else {
        return "nothing".to_owned();
    };
    if let Some(word) = to.as_str() {
        return word.to_owned();
    }
    if let Some(path) = text(to, "file") {
        return format!("file {path}");
    }
    if let Some(name) = text(to, "external") {
        return format!("external {name}");
    }
    if let Some(reason) = to
        .get("unresolved")
        .and_then(|unresolved| text(unresolved, "reason"))
    {
        return format!("unresolved {reason}");
    }
    to.to_string()
}

fn file<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    items(json, "files").find(|file| text(file, "path") == Some(path))
}

fn project<'a>(json: &'a Value, root: &str) -> &'a Value {
    items(json, "projects")
        .find(|project| text(project, "root") == Some(root))
        .unwrap_or(&Value::Null)
}

fn items<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Where the import of `spec` in the file at `path` leads, worded as `imports` words it after the arrow.
pub(crate) fn leads(json: &Value, path: &str, spec: &str) -> String {
    let wanted = format!("{spec} ");
    imports(json, path)
        .into_iter()
        .find(|line| line.starts_with(&wanted))
        .and_then(|line| line.split_once("-> ").map(|(_, to)| to.to_owned()))
        .unwrap_or_else(|| "no such import".to_owned())
}
