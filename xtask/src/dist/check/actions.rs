//! Workflow actions pinned by a full commit id.

use std::path::{Path, PathBuf};

use anyhow::Context as _;

/// Where the workflows are, from the repository root.
const WORKFLOWS: &str = ".github/workflows";

/// One problem for every `uses:` line of a workflow that does not name a full commit id, as `<file>:<line>: <what is wrong>`.
pub(super) fn problems(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut problems = Vec::new();
    for file in workflow_files(&root.join(WORKFLOWS))? {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("cannot read {}", file.display()))?;
        let name = file.strip_prefix(root).unwrap_or(&file).display();
        for (number, line) in (1_usize..).zip(text.lines()) {
            if let Some(reference) = uses(line)
                && !pinned(reference)
            {
                problems.push(format!(
                    "{name}:{number}: `uses: {reference}` is not pinned by a full commit id"
                ));
            }
        }
    }
    Ok(problems)
}

/// The workflow files in `directory`, sorted. No directory means no workflows.
fn workflow_files(directory: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", directory.display()));
        }
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("yml" | "yaml")
        ) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// What a `uses:` line refers to, without quotes and trailing comment, or `None` for any other line.
fn uses(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let line = line.strip_prefix("- ").map_or(line, str::trim_start);
    let value = line.strip_prefix("uses:")?;
    let value = value.split(" #").next().unwrap_or(value).trim();
    Some(value.trim_matches(['"', '\'']))
}

/// Whether `reference` is a local action or is pinned as `<action>@<40 hexadecimal digits>`.
fn pinned(reference: &str) -> bool {
    if reference.starts_with("./") {
        return true;
    }
    reference.rsplit_once('@').is_some_and(|(_, id)| {
        id.len() == 40
            && id
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}

#[cfg(test)]
mod tests {
    use super::{pinned, uses};

    #[test]
    fn reads_the_reference_of_a_uses_line() {
        assert_eq!(
            uses("      - uses: actions/checkout@v4"),
            Some("actions/checkout@v4")
        );
        assert_eq!(
            uses("        uses: \"actions/checkout@v4\" # the latest"),
            Some("actions/checkout@v4")
        );
        assert_eq!(uses("      - run: echo uses: nothing"), None);
        assert_eq!(uses("      # uses: actions/checkout@v4"), None);
    }

    #[test]
    fn accepts_only_a_full_commit_id() {
        let id = "3d3c42e5aac5ba805825da76410c181273ba90b1";
        assert!(pinned(&format!("actions/checkout@{id}")));
        assert!(pinned("./.github/actions/local"));
        assert!(!pinned("actions/checkout@v4"));
        assert!(!pinned("actions/checkout"));
        assert!(!pinned(
            "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b"
        ));
        assert!(!pinned(&format!("actions/checkout@{}", id.to_uppercase())));
    }
}
