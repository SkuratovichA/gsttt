//! Finding what a question names: a file by the path a person typed, and the unresolved imports that may have meant
//! a file the index does not hold.

use std::fmt;

use serde::Serialize;

use crate::fact::{Line, Resolution, UnresolvedReason};
use crate::ids::FileId;
use crate::index::Index;
use crate::path::RepoPath;

/// The file `raw` names, with its path. A file is in the index for a question only when a connector owns it, because
/// the graph lists every file of the tree and an answer about a file no connector claimed would state a certainty it
/// does not have. A file the connector owns but failed to parse is in the index, and its answers say so from its
/// parse status. When the index holds no such file, the error is the path to print for it: `raw` taken to its
/// canonical form when it has one.
pub(super) fn find<'a>(index: &'a Index, raw: &str) -> Result<(FileId, &'a RepoPath), String> {
    let owned = |path: &str| {
        let file = index.graph.file_id(path)?;
        index.project_of(file)?;
        Some((file, index.graph.path(file)?))
    };
    if let Some(found) = owned(raw) {
        return Ok(found);
    }
    match RepoPath::normalize(raw) {
        Ok(path) => owned(path.as_str()).ok_or_else(|| path.to_string()),
        Err(_) => Err(raw.to_owned()),
    }
}

/// An import that resolved to nothing and whose specifier ends like the name of a file, so that it may have
/// meant that file: a path alias the connector does not know, or an import of a file that has since been deleted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Miss {
    /// The file with the import.
    pub path: String,
    pub line: Line,
    /// The specifier as written.
    pub spec: String,
    pub reason: UnresolvedReason,
}

impl Miss {
    /// Puts `misses` in the order they are read in: by file, then line, then specifier. A connector lists the imports
    /// of a file in the order it found them, which is not always the order they stand in.
    pub(super) fn sort(misses: &mut [Self]) {
        misses.sort_by(|a, b| (&a.path, a.line, &a.spec).cmp(&(&b.path, b.line, &b.spec)));
    }
}

/// `src/a.ts:4 imports ./old (no such file)`.
impl fmt::Display for Miss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} imports {} ({})",
            self.path, self.line, self.spec, self.reason
        )
    }
}

/// The unresolved imports of the index whose specifier ends with the stem of one of `files`, in path and line order.
pub(super) fn possible_misses(index: &Index, files: &[&RepoPath]) -> Vec<Miss> {
    let stems: Vec<&str> = files
        .iter()
        .map(|file| stem(file))
        .filter(|stem| !stem.is_empty())
        .collect();
    if stems.is_empty() {
        return Vec::new();
    }
    let mut misses = Vec::new();
    for (file, path) in index.graph.files() {
        let Some(record) = index.record(file) else {
            continue;
        };
        for resolved in &record.imports {
            let Resolution::Unresolved(reason) = &resolved.to else {
                continue;
            };
            let spec = &resolved.import.spec;
            if stems.iter().any(|stem| ends_with_name(spec, stem)) {
                misses.push(Miss {
                    path: path.to_string(),
                    line: resolved.import.line,
                    spec: spec.clone(),
                    reason: reason.clone(),
                });
            }
        }
    }
    Miss::sort(&mut misses);
    misses
}

/// The file name without its last extension: `a.test` for `src/a.test.ts`.
fn stem(path: &RepoPath) -> &str {
    let name = path.file_name();
    path.extension()
        .and_then(|extension| name.strip_suffix(extension))
        .and_then(|stem| stem.strip_suffix('.'))
        .unwrap_or(name)
}

/// Whether `spec` ends with `name` as a whole name: `./old` and `pkg.old` end with `old`, `./gold` does not.
fn ends_with_name(spec: &str, name: &str) -> bool {
    spec.strip_suffix(name).is_some_and(|before| {
        before
            .chars()
            .next_back()
            .is_none_or(|last| !(last.is_alphanumeric() || last == '_' || last == '-'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_ends_a_specifier_at_a_boundary() {
        for spec in [
            "./old",
            "../lib/old",
            "pkg.old",
            "crate::old",
            "@/old",
            "old",
        ] {
            assert!(ends_with_name(spec, "old"), "{spec}");
        }
        for spec in ["./gold", "./old_", "./old/index", "./older", "./my-old"] {
            assert!(!ends_with_name(spec, "old"), "{spec}");
        }
    }

    #[test]
    fn stems_drop_the_last_extension_only() {
        let stem_of = |name: &str| stem(&RepoPath::normalize(name).unwrap()).to_owned();
        assert_eq!(stem_of("src/old.ts"), "old");
        assert_eq!(stem_of("src/a.test.ts"), "a.test");
        assert_eq!(stem_of(".env"), ".env");
        assert_eq!(stem_of("Makefile"), "Makefile");
    }
}
