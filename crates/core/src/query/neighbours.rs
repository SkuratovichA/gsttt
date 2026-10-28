//! `dependents` and `dependencies`: one level of the import graph around a file, with how each edge was written.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::fact::{ImportKind, Line, ParseStatus, Resolution};
use crate::graph::{Edge, Graph};
use crate::ids::FileId;
use crate::index::{Freshness, Index};
use crate::text::noun;

use super::lookup::{Miss, find, possible_misses};
use super::render::{Answer, Entries, Footer, Searched, display_as_text, list};

/// A file at the other end of an import edge, with how the imports between the two files were written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Neighbour {
    pub path: String,
    /// The syntaxes of the imports, in a fixed order.
    pub kinds: Vec<ImportKind>,
    /// Whether nothing behind the edge exists at run time.
    pub type_only: bool,
}

impl Neighbour {
    fn of(graph: &Graph, file: FileId, edge: Edge) -> Option<Self> {
        Some(Self {
            path: graph.path(file)?.to_string(),
            kinds: edge.kinds().collect(),
            type_only: edge.type_only(),
        })
    }
}

/// `b.ts  static import`, `c.ts  re-export of everything`, `d.ts  static import (types only)`.
impl fmt::Display for Neighbour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}  ", self.path)?;
        for (position, kind) in self.kinds.iter().enumerate() {
            if position > 0 {
                f.write_str(", ")?;
            }
            f.write_str(kind_label(*kind))?;
        }
        if self.type_only {
            f.write_str(" (types only)")?;
        }
        Ok(())
    }
}

/// How an import of this kind is written, for a sentence.
pub(super) const fn kind_label(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Static => "static import",
        ImportKind::ReExport => "re-export",
        ImportKind::ReExportAll => "re-export of everything",
        ImportKind::SideEffect => "side-effect import",
        ImportKind::Dynamic => "dynamic import",
        ImportKind::Require => "require",
        ImportKind::Reference => "reference",
        ImportKind::Module => "mod declaration",
        ImportKind::Include => "include",
    }
}

/// The files that import one file.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Dependents {
    /// The file asked about as the index names it, or as it was asked when the index does not hold it.
    pub path: String,
    /// Whether a connector owns the file, which is what it takes to be in the index. The connector may still have
    /// failed to parse it: importers are found from the importing files, so they stay true of such a file.
    pub in_index: bool,
    /// The importers, ordered by path.
    pub files: Vec<Neighbour>,
    /// When nothing imports the file: the unresolved imports that may have meant it.
    pub possible_misses: Vec<Miss>,
    /// What an answer that finds nothing searched.
    pub searched: Searched,
    pub footer: Footer,
}

/// The files that import `path`, one level, each with the kinds of its edge.
pub fn dependents(index: &Index, freshness: Freshness, path: &str) -> Dependents {
    let graph = &index.graph;
    let (file, name) = match find(index, path) {
        Ok(found) => found,
        Err(unknown) => {
            return Dependents {
                path: unknown,
                in_index: false,
                files: Vec::new(),
                possible_misses: Vec::new(),
                searched: Searched::of(index),
                footer: Footer::new(index, freshness, []),
            };
        }
    };
    let importers = graph.dependents(file);
    let files: Vec<Neighbour> = importers
        .iter()
        .filter_map(|&(importer, edge)| Neighbour::of(graph, importer, edge))
        .collect();
    let possible_misses = if files.is_empty() {
        possible_misses(index, &[name])
    } else {
        Vec::new()
    };
    Dependents {
        path: name.to_string(),
        in_index: true,
        files,
        possible_misses,
        searched: Searched::of(index),
        footer: Footer::new(
            index,
            freshness,
            std::iter::once(file).chain(importers.iter().map(|&(importer, _)| importer)),
        ),
    }
}

impl Answer for Dependents {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        if !self.in_index {
            writeln!(f, "{} is not in the index", self.path)?;
        } else if self.files.is_empty() {
            writeln!(f, "nothing imports {} among {}", self.path, self.searched)?;
            let misses = |f: &mut fmt::Formatter<'_>, miss: &Miss| write!(f, "{miss}");
            list(f, "possible misses", &self.possible_misses, entries, misses)?;
        } else {
            let heading = format!("dependents of {}", self.path);
            list(f, &heading, &self.files, entries, |f, file| {
                write!(f, "{file}")
            })?;
        }
        self.footer.write(f)
    }
}

/// A package or a builtin that a file imports, with the lines it does so on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct External {
    /// The registry name of a package, or the specifier of a builtin.
    pub name: String,
    pub lines: Vec<Line>,
}

/// What one file imports: the files, the packages, the builtins, and the imports that resolved to nothing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Dependencies {
    /// The file asked about as the index names it, or as it was asked when the index does not hold it.
    pub path: String,
    /// Whether a connector owns the file, which is what it takes to be in the index. The connector may still have
    /// failed to parse it, which `status` says.
    pub in_index: bool,
    /// The connector that owns the file.
    pub read_by: Option<String>,
    /// How the parse of the file went, `None` when the index does not hold the file. The imports below are all the
    /// file has only when this is [`ParseStatus::Ok`]: a failed parse leaves none, a recovered one may leave some out.
    pub status: Option<ParseStatus>,
    /// The files it imports, ordered by path.
    pub files: Vec<Neighbour>,
    /// Third-party packages, ordered by name.
    pub packages: Vec<External>,
    /// Builtins of the language or runtime, ordered by specifier.
    pub builtins: Vec<External>,
    /// Imports that point nowhere, with the reason, ordered by line.
    pub unresolved: Vec<Miss>,
    pub footer: Footer,
}

/// The files, packages and builtins that `path` imports, and the imports that resolved to nothing with why.
pub fn dependencies(index: &Index, freshness: Freshness, path: &str) -> Dependencies {
    let graph = &index.graph;
    let (file, name) = match find(index, path) {
        Ok(found) => found,
        Err(unknown) => {
            return Dependencies {
                path: unknown,
                in_index: false,
                read_by: None,
                status: None,
                files: Vec::new(),
                packages: Vec::new(),
                builtins: Vec::new(),
                unresolved: Vec::new(),
                footer: Footer::new(index, freshness, []),
            };
        }
    };
    let imported = graph.dependencies(file);
    let files = imported
        .iter()
        .filter_map(|&(imported, edge)| Neighbour::of(graph, imported, edge))
        .collect();

    let mut packages: BTreeMap<&str, Vec<Line>> = BTreeMap::new();
    let mut builtins: BTreeMap<&str, Vec<Line>> = BTreeMap::new();
    let mut unresolved = Vec::new();
    let record = index.record(file);
    for resolved in record.map_or(&[][..], |record| &record.imports) {
        let import = &resolved.import;
        match &resolved.to {
            Resolution::External { package } => {
                packages.entry(package).or_default().push(import.line);
            }
            Resolution::Builtin => builtins.entry(&import.spec).or_default().push(import.line),
            Resolution::Unresolved(reason) => unresolved.push(Miss {
                path: name.to_string(),
                line: import.line,
                spec: import.spec.clone(),
                reason: reason.clone(),
            }),
            // The graph holds these as edges, with the kinds merged per file.
            Resolution::File { .. } | Resolution::Files { .. } => {}
        }
    }
    unresolved.sort_by(|a, b| (a.line, &a.spec).cmp(&(b.line, &b.spec)));

    Dependencies {
        path: name.to_string(),
        in_index: true,
        read_by: index
            .project_of(file)
            .map(|project| project.connector.clone()),
        status: record.map(|record| record.status.clone()),
        files,
        packages: externals(packages),
        builtins: externals(builtins),
        unresolved,
        footer: Footer::new(
            index,
            freshness,
            std::iter::once(file).chain(imported.iter().map(|&(imported, _)| imported)),
        ),
    }
}

fn externals(found: BTreeMap<&str, Vec<Line>>) -> Vec<External> {
    found
        .into_iter()
        .map(|(name, mut lines)| {
            lines.sort_unstable();
            lines.dedup();
            External {
                name: name.to_owned(),
                lines,
            }
        })
        .collect()
}

impl Dependencies {
    /// What the parse status of the file means for the lists, when it is not a clean read: a file that failed to
    /// parse has no imports to list, and one read past syntax errors may have lost some, so neither may be told as
    /// a file that imports nothing.
    fn caveat(&self) -> Option<String> {
        match self.status.as_ref()? {
            ParseStatus::Ok => None,
            ParseStatus::Recovered {
                errors,
                first_error_line,
            } => Some(format!(
                "{} was read past {}, the first at line {first_error_line}, so imports may be missing",
                self.path,
                noun(*errors, "syntax error"),
            )),
            ParseStatus::Failed(failure) => Some(format!(
                "{} could not be parsed, so its imports are unknown: {failure}",
                self.path,
            )),
        }
    }
}

impl Answer for Dependencies {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        let caveat = self.caveat();
        if !self.in_index {
            writeln!(f, "{} is not in the index", self.path)?;
        } else if self.files.is_empty()
            && self.packages.is_empty()
            && self.builtins.is_empty()
            && self.unresolved.is_empty()
        {
            if let Some(caveat) = caveat {
                writeln!(f, "{caveat}")?;
            } else {
                write!(f, "{} imports nothing", self.path)?;
                if let Some(connector) = &self.read_by {
                    write!(f, "; {connector} found no imports in it")?;
                }
                f.write_str("\n")?;
            }
        } else {
            writeln!(f, "dependencies of {}", self.path)?;
            if let Some(caveat) = caveat {
                writeln!(f, "{caveat}")?;
            }
            list(f, "files", &self.files, entries, |f, file| {
                write!(f, "{file}")
            })?;
            let sites = |f: &mut fmt::Formatter<'_>, external: &External| {
                write!(f, "{}  ", external.name)?;
                for (position, line) in external.lines.iter().enumerate() {
                    if position > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}:{line}", self.path)?;
                }
                Ok(())
            };
            list(f, "packages", &self.packages, entries, sites)?;
            list(f, "builtins", &self.builtins, entries, sites)?;
            list(f, "unresolved", &self.unresolved, entries, |f, miss| {
                write!(f, "{miss}")
            })?;
        }
        self.footer.write(f)
    }
}

display_as_text!(Dependents, Dependencies);
