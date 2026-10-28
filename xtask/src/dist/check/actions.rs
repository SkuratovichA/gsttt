//! Workflow and composite-action references pinned by a full commit id or an image digest.
//!
//! The parsed YAML says which `uses` are real, and a scan of the lines says where they are. A scan of lines alone misses a `uses` in a flow mapping or behind a quoted key, and reports one in the text of a `run: |` script.

use std::path::Path;

use anyhow::Context as _;
use gsttt_spec::{Entry, read_directory};
use serde_norway::{Mapping, Value};

use crate::digest::is_hex;

/// Where the workflows are, from the repository root.
const WORKFLOWS: &str = ".github/workflows";

/// Where the composite actions of the repository are, from the repository root.
const ACTIONS: &str = ".github/actions";

/// One problem for every `uses` of a workflow or of a composite action that is not pinned, as `<file>:<line>: <what is wrong>`, and one when the repository has no workflows.
pub(super) fn problems(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut files = Vec::new();
    match read_directory(&root.join(WORKFLOWS)).context("listing the workflows")? {
        Some(entries) => files.extend(entries.into_iter().filter(|entry| is_yaml(&entry.path))),
        None => problems.push(format!(
            "{WORKFLOWS}: the directory is missing, so no workflow is checked"
        )),
    }
    files.extend(composite_actions(&root.join(ACTIONS))?);
    for file in files {
        problems.extend(file_problems(root, &file.path)?);
    }
    Ok(problems)
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yml" | "yaml")
    )
}

/// The `action.yml` files below `directory`, however deep. No directory means no composite actions.
fn composite_actions(directory: &Path) -> anyhow::Result<Vec<Entry>> {
    let mut found = Vec::new();
    for entry in read_directory(directory)
        .context("listing the composite actions")?
        .unwrap_or_default()
    {
        if entry.kind.is_dir() {
            found.extend(composite_actions(&entry.path)?);
        } else if matches!(
            entry.path.file_name().and_then(|name| name.to_str()),
            Some("action.yml" | "action.yaml")
        ) {
            found.push(entry);
        }
    }
    Ok(found)
}

/// One problem for every real `uses` of `file` that is not pinned.
fn file_problems(root: &Path, file: &Path) -> anyhow::Result<Vec<String>> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let name = file.strip_prefix(root).unwrap_or(file).display();
    let document: Value = match serde_norway::from_str(&text) {
        Ok(document) => document,
        Err(error) => return Ok(vec![format!("{name}: is not valid YAML: {error}")]),
    };
    let mut lines = uses_lines(&text);
    let mut problems = Vec::new();
    for reference in references(&document) {
        if pinned(reference) {
            continue;
        }
        let what = format!(
            "`uses: {reference}` is not pinned by a full commit id (a Docker image: by a sha256 digest)"
        );
        // A reference whose line cannot be found is still reported, without a line.
        match lines.iter().position(|(_, value)| *value == reference) {
            Some(index) => {
                let (number, _) = lines.remove(index);
                problems.push(format!("{name}:{number}: {what}"));
            }
            None => problems.push(format!("{name}: {what}")),
        }
    }
    Ok(problems)
}

/// Every `uses` of the workflow or composite action `document`, in the order of the file: of the jobs that call a reusable workflow, and of the steps of jobs and of `runs`.
fn references(document: &Value) -> Vec<&str> {
    let jobs = document
        .get("jobs")
        .and_then(Value::as_mapping)
        .into_iter()
        .flat_map(Mapping::values);
    jobs.chain(document.get("runs"))
        .flat_map(|holder| {
            let steps = holder
                .get("steps")
                .and_then(Value::as_sequence)
                .into_iter()
                .flatten()
                .filter_map(|step| step.get("uses"));
            holder.get("uses").into_iter().chain(steps)
        })
        .filter_map(Value::as_str)
        .collect()
}

/// Whether `reference` is a local action, is pinned as `<action>@<40 hexadecimal digits>`, or is a Docker image pinned as `docker://<image>@sha256:<64 hexadecimal digits>`.
fn pinned(reference: &str) -> bool {
    if reference.starts_with("./") {
        return true;
    }
    let Some((_, id)) = reference.rsplit_once('@') else {
        return false;
    };
    if reference.starts_with("docker://") {
        return id
            .strip_prefix("sha256:")
            .is_some_and(|digest| is_hex(digest, 64));
    }
    is_hex(id, 40)
}

/// The lines of `text` that hold a `uses` key, as the line number and the value. Comments and the lines of block scalars (`run: |`) are skipped.
fn uses_lines(text: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    // The column of the key whose block scalar is being skipped.
    let mut block = None;
    for (number, line) in (1_usize..).zip(text.lines()) {
        let content = line.trim_start();
        if let Some(column) = block {
            if content.is_empty() || line.len() - content.len() > column {
                continue;
            }
            block = None;
        }
        if content.starts_with('#') {
            continue;
        }
        block = block_scalar_column(line);
        found.extend(
            uses_values(content)
                .into_iter()
                .map(|value| (number, value)),
        );
    }
    found
}

/// The column of the key when `line` ends with a block scalar header, as in `run: |` or `- run: >-`. The lines of the block are indented deeper than that column.
fn block_scalar_column(line: &str) -> Option<usize> {
    let code = line.split(" #").next().unwrap_or(line).trim_end();
    let (key, header) = code.rsplit_once(' ')?;
    let indicators = header.strip_prefix(['|', '>'])?;
    if !key.ends_with(':')
        || !indicators
            .chars()
            .all(|c| matches!(c, '+' | '-' | '0'..='9'))
    {
        return None;
    }
    Some(line.len() - line.trim_start_matches(['-', ' ']).len())
}

/// The values of the `uses` keys on a line of YAML without its indentation: `uses: x`, `- uses: x`, `"uses": "x"`, and each one in a flow mapping such as `- { name: n, uses: x }`.
fn uses_values(content: &str) -> Vec<&str> {
    content
        .match_indices("uses")
        .filter_map(|(start, key)| {
            let before = content.get(..start)?;
            let after = content.get(start + key.len()..)?;
            let (before, after) = ['"', '\'']
                .into_iter()
                .find_map(|quote| Some((before.strip_suffix(quote)?, after.strip_prefix(quote)?)))
                .unwrap_or((before, after));
            // A key starts a line, a sequence item or a flow mapping entry.
            let before = before.trim_end();
            if !(before.is_empty() || before.ends_with(['-', '{', ','])) {
                return None;
            }
            let value = after.trim_start().strip_prefix(':')?;
            if !(value.is_empty() || value.starts_with(char::is_whitespace)) {
                return None;
            }
            let value = value.trim_start();
            let value = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value.get(1..)?.split(quote).next()?,
                _ => value
                    .split([',', '}'])
                    .next()?
                    .split(" #")
                    .next()?
                    .trim_end(),
            };
            (!value.is_empty()).then_some(value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{block_scalar_column, pinned, references, uses_lines, uses_values};

    #[test]
    fn reads_the_values_of_the_uses_keys_on_a_line() {
        assert_eq!(
            uses_values("- uses: actions/checkout@v4"),
            ["actions/checkout@v4"]
        );
        assert_eq!(
            uses_values("uses: \"actions/checkout@v4\" # the latest"),
            ["actions/checkout@v4"]
        );
        assert_eq!(
            uses_values("- 'uses': 'actions/checkout@v4'"),
            ["actions/checkout@v4"]
        );
        assert_eq!(
            uses_values("- { name: checkout, uses: actions/checkout@v4 }"),
            ["actions/checkout@v4"]
        );
        assert_eq!(
            uses_values("{uses: a/b@v1, with: { uses: c/d@v2 }}"),
            ["a/b@v1", "c/d@v2"]
        );
    }

    #[test]
    fn ignores_text_that_only_looks_like_a_uses_key() {
        let nothing: [&str; 0] = [];
        for line in [
            "- run: echo uses: nothing",
            "- name: uses actions/checkout@v4",
            "uses:actions/checkout@v4",
            "reuses: actions/checkout@v4",
            "uses:",
        ] {
            assert_eq!(uses_values(line), nothing, "{line}");
        }
    }

    #[test]
    fn skips_comments_and_the_lines_of_block_scalars() {
        let text = "\
steps:
  # uses: a/b@v1
  - uses: c/d@v2
  - run: |
      uses: e/f@v3

      uses: g/h@v4
  - name: after
    uses: i/j@v5
";
        assert_eq!(
            uses_lines(text),
            [(3, "c/d@v2"), (9, "i/j@v5")],
            "the lines of the script and the comment are not actions"
        );
    }

    #[test]
    fn finds_the_key_column_of_a_block_scalar() {
        assert_eq!(block_scalar_column("    run: |"), Some(4));
        assert_eq!(block_scalar_column("      - run: >-"), Some(8));
        assert_eq!(block_scalar_column("      - run: |2+ # keep"), Some(8));
        assert_eq!(block_scalar_column("    run: echo a | b"), None);
        assert_eq!(block_scalar_column("    run: cat |"), None);
        assert_eq!(block_scalar_column("    run: |x"), None);
    }

    #[test]
    fn reads_the_references_of_workflows_and_composite_actions_only() {
        let workflow: serde_norway::Value = serde_norway::from_str(
            "
jobs:
  call:
    uses: o/r/.github/workflows/x.yml@v1
  build:
    steps:
      - uses: a/b@v1
      - run: echo
        env: { uses: not/a-step@v1 }
      - uses: c/d@v2
        with: { uses: not/an-input@v1 }
",
        )
        .unwrap();
        assert_eq!(
            references(&workflow),
            ["o/r/.github/workflows/x.yml@v1", "a/b@v1", "c/d@v2"]
        );

        let composite: serde_norway::Value =
            serde_norway::from_str("runs:\n  using: composite\n  steps:\n    - uses: e/f@v3\n")
                .unwrap();
        assert_eq!(references(&composite), ["e/f@v3"]);
    }

    #[test]
    fn accepts_only_a_full_commit_id_or_an_image_digest() {
        let id = "3d3c42e5aac5ba805825da76410c181273ba90b1";
        let digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(pinned(&format!("actions/checkout@{id}")));
        assert!(pinned("./.github/actions/local"));
        assert!(pinned(&format!("docker://alpine@{digest}")));
        assert!(pinned(&format!("o/r/.github/workflows/x.yml@{id}")));
        assert!(!pinned("actions/checkout@v4"));
        assert!(!pinned("actions/checkout"));
        assert!(!pinned(
            "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b"
        ));
        assert!(!pinned(&format!("actions/checkout@{}", id.to_uppercase())));
        assert!(!pinned("docker://alpine:3.19"));
        assert!(!pinned(&format!("docker://alpine@{id}")));
        assert!(!pinned("docker://alpine@sha256:0123"));
        assert!(!pinned(&format!("actions/checkout@{digest}")));
    }
}
