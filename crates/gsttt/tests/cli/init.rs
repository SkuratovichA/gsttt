//! `gsttt init`: the starter config, and the schemas an editor checks it with.

use std::path::Path;

use gsttt_scenario::scenario;
use serde_json::Value;

use super::support::{Sandbox, init_repo, json, write};

const CONFIG: &str = ".gsttt/config.toml";
const CONFIG_SCHEMA: &str = ".gsttt/schema/config.json";
const PACK_SCHEMA: &str = ".gsttt/schema/pack.json";

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap()
}

#[scenario("cli", "init writes the starter files")]
#[test]
fn init_writes_the_starter_files() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(&root, "package.json", "{}\n");
    write(&root, "tsconfig.json", "{}\n");

    let ran = sandbox.run_in(&root, &["init"]);

    assert_eq!(ran.code, Some(0), "{ran:?}");
    assert_eq!(ran.stderr, "", "{ran:?}");
    let config = read(&root, CONFIG);
    assert_eq!(config.lines().next(), Some("#:schema ./schema/config.json"));
    assert!(config.lines().any(|line| line.contains("[ts]")), "{config}");
    for schema in [CONFIG_SCHEMA, PACK_SCHEMA] {
        let text = read(&root, schema);
        assert!(text.ends_with("}\n"), "{schema}: not newline-terminated");
        assert!(text.contains("\n  \""), "{schema}: not indented");
        let document: Value = serde_json::from_str(&text).unwrap();
        let comment = document["$comment"].as_str().unwrap();
        assert!(comment.contains("gsttt init"), "{schema}: {comment}");
        assert!(comment.contains("lost"), "{schema}: {comment}");
    }
    assert_eq!(
        ran.stdout,
        format!("wrote {CONFIG}\nwrote {CONFIG_SCHEMA}\nwrote {PACK_SCHEMA}\n")
    );
}

#[scenario("cli", "init keeps the config and rewrites the schemas")]
#[test]
fn init_keeps_the_config_and_rewrites_the_schemas() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    assert_eq!(sandbox.run_in(&root, &["init"]).code, Some(0));
    let generated = read(&root, CONFIG_SCHEMA);
    let edited = format!("{}max_file_bytes = 1024\n", read(&root, CONFIG));
    write(&root, CONFIG, &edited);
    write(&root, CONFIG_SCHEMA, "{\"edited\": true}\n");

    let again = sandbox.run_in(&root, &["init"]);

    assert_eq!(again.code, Some(0), "{again:?}");
    assert_eq!(read(&root, CONFIG), edited);
    assert_eq!(read(&root, CONFIG_SCHEMA), generated);
    assert_eq!(
        again.stdout,
        format!("kept {CONFIG}\nwrote {CONFIG_SCHEMA}\nwrote {PACK_SCHEMA}\n")
    );
}

/// `init` does not read the config, so it works beside one that does not parse, and leaves it as it is.
#[test]
fn init_works_beside_a_config_that_does_not_parse() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(&root, CONFIG, "[ts\n");

    let ran = sandbox.run_in(&root, &["init"]);

    assert_eq!(ran.code, Some(0), "{ran:?}");
    assert_eq!(read(&root, CONFIG), "[ts\n");
    assert!(
        ran.stdout.starts_with(&format!("kept {CONFIG}\n")),
        "{ran:?}"
    );
    assert!(root.join(PACK_SCHEMA).is_file());
}

/// The starter config is one the commands accept, and the schema it names is the one `init` wrote next to it.
#[test]
fn the_starter_config_is_valid() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    assert_eq!(sandbox.run_in(&root, &["init"]).code, Some(0));

    let checked = sandbox.run_in(&root, &["config", "check"]);

    assert_eq!(checked.code, Some(0), "{checked:?}");
    assert_eq!(checked.stdout, "The config is valid.\n");
    let shown = json(&sandbox.run_in(&root, &["--json", "config", "show"]));
    assert_eq!(shown["detect"], true);
}

/// The modules the starter names are the ones that find a project in the tree, not a fixed list. In a tree without code those are `graphql` and `sql`, whose connectors detect a project in every tree, because code in any language can embed their documents.
#[test]
fn the_starter_names_only_the_modules_that_detect_in_a_tree_without_code() {
    let sandbox = Sandbox::new();
    let root = sandbox.work.path().join("docs");
    std::fs::create_dir_all(&root).unwrap();
    init_repo(&root);
    write(&root, "README.md", "# docs\n");

    let ran = sandbox.run_in(&root, &["init"]);

    assert_eq!(ran.code, Some(0), "{ran:?}");
    let config = read(&root, CONFIG);
    let named: Vec<&str> = config
        .lines()
        .filter(|line| line.starts_with("# ["))
        .collect();
    assert_eq!(named, ["# [graphql]", "# [sql]"], "{config}");
    assert!(!config.contains("No module was detected"), "{config}");
}

/// The schema of the config lists the modules of the registry, so that an editor offers their options.
#[test]
fn the_config_schema_describes_the_options_of_the_modules() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    assert_eq!(sandbox.run_in(&root, &["init"]).code, Some(0));

    let schema = read(&root, CONFIG_SCHEMA);

    assert!(schema.contains("\"ts\""), "{schema}");
    assert!(schema.contains("tsconfig"), "{schema}");
}

/// No description in the schemas breaks a line inside a paragraph: `schemars` keeps the line breaks of a doc comment, so a wrapped one reaches the schemas `init` writes and an editor shows it broken.
#[test]
fn no_description_in_the_schemas_breaks_inside_a_paragraph() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    assert_eq!(sandbox.run_in(&root, &["init"]).code, Some(0));

    let mut broken = Vec::new();
    for schema in [CONFIG_SCHEMA, PACK_SCHEMA] {
        let document: Value = serde_json::from_str(&read(&root, schema)).unwrap();
        let all = descriptions(&document);
        assert!(!all.is_empty(), "{schema}: the walk finds no description");
        broken.extend(
            all.into_iter()
                .filter(|description| breaks_inside_a_paragraph(description))
                .map(|description| format!("{schema}: {description:?}")),
        );
    }

    assert!(
        broken.is_empty(),
        "{} descriptions break inside a paragraph:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

/// Every text under `value` that is a description. The walk of `crates/mcp/tests/server/listing.rs` is the same, and a test of another crate cannot import it.
fn descriptions(value: &Value) -> Vec<&str> {
    match value {
        Value::Object(members) => members
            .iter()
            .flat_map(|(name, member)| match member {
                Value::String(text) if name == "description" => vec![text.as_str()],
                _ => descriptions(member),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(descriptions).collect(),
        _ => Vec::new(),
    }
}

/// Whether a line of text sits right under another one, outside a code fence, and opens no list item, heading, table row, quote or fence: the paragraph above it goes on in a new line.
fn breaks_inside_a_paragraph(description: &str) -> bool {
    let mut fenced = false;
    let mut above = false;
    for line in description.lines().map(str::trim_start) {
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            above = false;
        } else if !fenced {
            if above && !line.is_empty() && !opens_a_block(line) {
                return true;
            }
            above = !line.is_empty();
        }
    }
    false
}

/// Whether a line opens a block of its own instead of going on with a paragraph: a list item, a heading, a table row or a quote.
fn opens_a_block(line: &str) -> bool {
    let digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
    let numbered =
        digits.len() < line.len() && (digits.starts_with(". ") || digits.starts_with(") "));
    numbered
        || ["- ", "* ", "+ ", "#", "|", ">"]
            .iter()
            .any(|opener| line.starts_with(opener))
}

/// A schema file that is a link is replaced by a file, and what the link led to stays as it was: a repository cannot make `init` overwrite a file elsewhere.
#[cfg(unix)]
#[test]
fn init_replaces_a_linked_schema_and_does_not_write_through_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    let precious = sandbox.work.path().join("precious");
    std::fs::write(&precious, "keep me\n").unwrap();
    std::fs::create_dir_all(root.join(".gsttt/schema")).unwrap();
    std::os::unix::fs::symlink(&precious, root.join(CONFIG_SCHEMA)).unwrap();

    let ran = sandbox.run_in(&root, &["init"]);

    assert_eq!(ran.code, Some(0), "{ran:?}");
    assert_eq!(std::fs::read_to_string(&precious).unwrap(), "keep me\n");
    let written = root.join(CONFIG_SCHEMA).symlink_metadata().unwrap();
    assert!(written.is_file(), "the link is still there");
}

/// A file that cannot be written ends the run with its name, and exit code 1.
#[test]
fn init_names_the_file_it_cannot_write() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    // A directory where a schema file belongs cannot be replaced by a file.
    std::fs::create_dir_all(root.join(CONFIG_SCHEMA).join("in-the-way")).unwrap();

    let ran = sandbox.run_in(&root, &["init"]);

    assert_eq!(ran.code, Some(1), "{ran:?}");
    assert!(ran.stderr.contains(CONFIG_SCHEMA), "{ran:?}");
}
