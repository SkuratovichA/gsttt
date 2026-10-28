//! What a run could see, and what it could not.
//!
//! Every answer carries its coverage, because "nothing imports this file" means something only next to "41 dynamic imports are computed at run time". A file a connector could not read, a file no connector reads, an import that points nowhere: each becomes a count here instead of a silent hole.

use serde::{Deserialize, Serialize};

use crate::connector::Provide;
use crate::fact::{
    Line, ParseFailure, ParseStatus, Resolution, ResolvedImport, UnresolvedReason, UnresolvedSite,
};
use crate::ids::FileId;
use crate::path::RepoPath;

/// The coverage of one index.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Coverage {
    /// One entry per connector that ran, in registry order.
    pub connectors: Vec<ConnectorCoverage>,
    /// Files no connector read, by extension, most first.
    pub unclaimed: Vec<Unclaimed>,
    /// Files left out before any connector saw them: over the size limit, not UTF-8, unreadable.
    pub skipped: Vec<FailedFile>,
    /// What the run could not do, one line each: a connector call that failed outside the parse of one file (opening a project, preparing, linking), a HEAD or a history that git could not read, a directory that could not be read. The run went on without what they would have added. The program that embeds the engine may add lines of its own, such as an external connector that waits for trust; the coverage answer lists all of them, whatever it cuts of its other lists.
    #[serde(default)]
    pub problems: Vec<String>,
}

/// What one connector read.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ConnectorCoverage {
    pub connector: String,
    /// The roots of its projects, `.` for the repository root, the way the engine writes a project root to people.
    pub projects: Vec<String>,
    pub files: u32,
    /// Files read past syntax errors.
    pub recovered: u32,
    /// Files it could not read, in path order.
    pub failed: Vec<FailedFile>,
    pub imports: u32,
    /// Imports that point at files of the repository.
    pub internal: u32,
    /// Imports of packages and of the language's own modules.
    pub external: u32,
    /// Imports that point nowhere, by reason, most first.
    pub unresolved: Vec<ReasonCount>,
    /// Sites it saw but could not read, by reason, most first: an import computed at run time, a URL built from variables.
    pub blind: Vec<ReasonCount>,
    pub provides: Vec<Provide>,
}

/// A file that was not read, and why.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FailedFile {
    pub file: FileId,
    pub failure: ParseFailure,
}

/// How often one reason came up, and where first.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ReasonCount {
    pub reason: UnresolvedReason,
    pub count: u32,
    /// The first place in path order, so an answer can show one example.
    pub first: Place,
}

/// A line of a file.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Place {
    pub file: FileId,
    pub line: Line,
}

/// Files with one extension that no connector read.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Unclaimed {
    /// The extension without its dot, `""` for files without one.
    pub extension: String,
    pub files: u32,
}

impl Unclaimed {
    /// The programming language the extension belongs to, when it is one a reader expects a code graph to cover.
    pub fn language(&self) -> Option<&'static str> {
        language_of(&self.extension)
    }
}

/// Programming languages by extension, sorted by extension, so coverage can name the code nothing read: "Kotlin: 120 files, no connector".
const LANGUAGES: &[(&str, &str)] = &[
    ("c", "C"),
    ("cc", "C++"),
    ("clj", "Clojure"),
    ("cpp", "C++"),
    ("cs", "C#"),
    ("cts", "TypeScript"),
    ("cxx", "C++"),
    ("dart", "Dart"),
    ("erl", "Erlang"),
    ("ex", "Elixir"),
    ("exs", "Elixir"),
    ("fs", "F#"),
    ("go", "Go"),
    ("groovy", "Groovy"),
    ("h", "C/C++"),
    ("hpp", "C++"),
    ("hs", "Haskell"),
    ("java", "Java"),
    ("jl", "Julia"),
    ("js", "JavaScript"),
    ("jsx", "JavaScript"),
    ("kt", "Kotlin"),
    ("kts", "Kotlin"),
    ("lua", "Lua"),
    ("m", "Objective-C"),
    ("mjs", "JavaScript"),
    ("ml", "OCaml"),
    ("mm", "Objective-C++"),
    ("mts", "TypeScript"),
    ("php", "PHP"),
    ("pl", "Perl"),
    ("py", "Python"),
    ("pyi", "Python"),
    ("r", "R"),
    ("rb", "Ruby"),
    ("rs", "Rust"),
    ("scala", "Scala"),
    ("sh", "Shell"),
    ("sol", "Solidity"),
    ("svelte", "Svelte"),
    ("swift", "Swift"),
    ("ts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("vue", "Vue"),
    ("zig", "Zig"),
];

/// The programming language of a file extension, for coverage lines.
pub fn language_of(extension: &str) -> Option<&'static str> {
    LANGUAGES
        .binary_search_by(|(known, _)| (*known).cmp(extension))
        .ok()
        .and_then(|index| LANGUAGES.get(index))
        .map(|(_, language)| *language)
}

impl Coverage {
    pub fn connector(&self, name: &str) -> Option<&ConnectorCoverage> {
        self.connectors
            .iter()
            .find(|coverage| coverage.connector == name)
    }

    /// Counts a file that no connector read.
    pub fn count_unclaimed(&mut self, path: &RepoPath) {
        let extension = path.extension().unwrap_or("");
        match self
            .unclaimed
            .iter_mut()
            .find(|unclaimed| unclaimed.extension == extension)
        {
            Some(unclaimed) => unclaimed.files = unclaimed.files.saturating_add(1),
            None => self.unclaimed.push(Unclaimed {
                extension: extension.to_owned(),
                files: 1,
            }),
        }
    }

    /// Puts every list in the order answers show it. Run it again after adding a line to `problems`.
    pub fn finish(&mut self) {
        self.problems.sort();
        self.problems.dedup();
        self.unclaimed.sort_unstable_by(|a, b| {
            b.files
                .cmp(&a.files)
                .then_with(|| a.extension.cmp(&b.extension))
        });
        self.skipped.sort_unstable_by_key(|failed| failed.file);
        for connector in &mut self.connectors {
            connector.finish();
        }
    }
}

impl ConnectorCoverage {
    pub fn new(connector: &str) -> Self {
        Self {
            connector: connector.to_owned(),
            ..Self::default()
        }
    }

    /// Counts a file the connector owns, by how it parsed.
    pub fn count_file(&mut self, file: FileId, status: &ParseStatus) {
        self.files = self.files.saturating_add(1);
        match status {
            ParseStatus::Ok => {}
            ParseStatus::Recovered { .. } => self.recovered = self.recovered.saturating_add(1),
            ParseStatus::Failed(failure) => self.failed.push(FailedFile {
                file,
                failure: failure.clone(),
            }),
        }
    }

    /// Counts an import of `file`, by where it resolved.
    pub fn count_import(&mut self, file: FileId, import: &ResolvedImport) {
        self.imports = self.imports.saturating_add(1);
        match &import.to {
            Resolution::File { .. } | Resolution::Files { .. } => {
                self.internal = self.internal.saturating_add(1);
            }
            Resolution::External { .. } | Resolution::Builtin => {
                self.external = self.external.saturating_add(1);
            }
            Resolution::Unresolved(reason) => {
                tally(
                    &mut self.unresolved,
                    reason,
                    Place {
                        file,
                        line: import.import.line,
                    },
                );
            }
        }
    }

    /// Counts a site of `file` the connector saw but could not read.
    pub fn count_blind(&mut self, file: FileId, site: &UnresolvedSite) {
        tally(
            &mut self.blind,
            &site.reason,
            Place {
                file,
                line: site.line,
            },
        );
    }

    /// Adds another tally of the same connector, such as one made by another thread.
    pub fn merge(&mut self, other: Self) {
        self.files = self.files.saturating_add(other.files);
        self.recovered = self.recovered.saturating_add(other.recovered);
        self.failed.extend(other.failed);
        self.imports = self.imports.saturating_add(other.imports);
        self.internal = self.internal.saturating_add(other.internal);
        self.external = self.external.saturating_add(other.external);
        for count in other.unresolved {
            add(&mut self.unresolved, count);
        }
        for count in other.blind {
            add(&mut self.blind, count);
        }
    }

    /// The share of imports that resolved to a file or a package, from 0 to 1; 1 when there are none.
    pub fn resolved_share(&self) -> f64 {
        if self.imports == 0 {
            return 1.0;
        }
        f64::from(self.internal.saturating_add(self.external)) / f64::from(self.imports)
    }

    fn finish(&mut self) {
        self.failed.sort_unstable_by_key(|failed| failed.file);
        for counts in [&mut self.unresolved, &mut self.blind] {
            counts
                .sort_unstable_by(|a, b| b.count.cmp(&a.count).then_with(|| a.first.cmp(&b.first)));
        }
    }
}

fn tally(counts: &mut Vec<ReasonCount>, reason: &UnresolvedReason, place: Place) {
    match counts.iter_mut().find(|count| count.reason == *reason) {
        Some(count) => {
            count.count = count.count.saturating_add(1);
            count.first = count.first.min(place);
        }
        None => counts.push(ReasonCount {
            reason: reason.clone(),
            count: 1,
            first: place,
        }),
    }
}

fn add(counts: &mut Vec<ReasonCount>, other: ReasonCount) {
    match counts.iter_mut().find(|count| count.reason == other.reason) {
        Some(count) => {
            count.count = count.count.saturating_add(other.count);
            count.first = count.first.min(other.first);
        }
        None => counts.push(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fact::{Import, ImportKind};

    fn unresolved(spec: &str, line: Line, reason: UnresolvedReason) -> ResolvedImport {
        ResolvedImport {
            import: Import::new(spec, ImportKind::Static, line),
            to: Resolution::Unresolved(reason),
        }
    }

    #[test]
    fn languages_are_sorted_for_binary_search() {
        assert!(LANGUAGES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(language_of("kt"), Some("Kotlin"));
        assert_eq!(language_of("md"), None);
    }

    #[test]
    fn tallies_merge_and_keep_the_first_place() {
        let (a, b) = (FileId::from_raw(1), FileId::from_raw(2));
        let mut left = ConnectorCoverage::new("ts");
        left.count_file(b, &ParseStatus::Ok);
        left.count_import(b, &unresolved("./x", 4, UnresolvedReason::NotFound));
        let mut right = ConnectorCoverage::new("ts");
        right.count_file(a, &ParseStatus::Failed(ParseFailure::NotUtf8));
        right.count_import(a, &unresolved("./y", 9, UnresolvedReason::NotFound));
        right.count_import(a, &unresolved("./z", 2, UnresolvedReason::NonLiteral));
        right.count_import(
            a,
            &ResolvedImport {
                import: Import::new("react", ImportKind::Static, 1),
                to: Resolution::External {
                    package: "react".into(),
                },
            },
        );
        left.merge(right);

        let mut coverage = Coverage {
            connectors: vec![left],
            ..Coverage::default()
        };
        coverage.finish();
        let ts = coverage.connector("ts").unwrap();
        assert_eq!(
            (ts.files, ts.failed.len(), ts.imports, ts.external),
            (2, 1, 4, 1)
        );
        assert_eq!(ts.unresolved[0].reason, UnresolvedReason::NotFound);
        assert_eq!(
            (ts.unresolved[0].count, ts.unresolved[0].first),
            (2, Place { file: a, line: 9 })
        );
        assert!((ts.resolved_share() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn unclaimed_files_count_by_extension() {
        let mut coverage = Coverage::default();
        for name in ["a.kt", "b.kt", "Makefile", "c.md"] {
            coverage.count_unclaimed(&RepoPath::normalize(name).unwrap());
        }
        coverage.finish();
        let first = &coverage.unclaimed[0];
        assert_eq!(
            (first.extension.as_str(), first.files, first.language()),
            ("kt", 2, Some("Kotlin"))
        );
        assert_eq!(coverage.unclaimed.len(), 3);
    }
}
