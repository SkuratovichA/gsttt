//! Where an import points: a module of the project, the standard library, a declared dependency or nothing.
//!
//! Resolution never touches the disk. `prepare` hands the project its parsed files, and the index built from them
//! answers every lookup.

use std::collections::{HashMap, HashSet};

use gsttt_core::connector::PrepareFile;
use gsttt_core::fact::{Import, Resolution, UnresolvedReason};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use ruff_python_stdlib::sys::{is_builtin_module, is_known_standard_library};

use crate::files::is_python_source;
use crate::names::distributions_for_import;
use crate::project::PythonProject;

/// The Python files of one project, by path, and the directories they lie in at any depth. The maps are only
/// ever looked up, never iterated.
#[derive(Debug, Default)]
pub(crate) struct ModuleIndex {
    files: HashMap<String, FileId>,
    dirs: HashSet<String>,
}

/// What the index holds at one module path under one directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spot {
    /// A module file or a package's `__init__` file.
    File(FileId),
    /// A directory with Python files below it and no `__init__` file: a namespace package.
    Namespace,
    Missing,
}

impl ModuleIndex {
    pub(crate) fn build(files: &[PrepareFile<'_>]) -> Self {
        let mut index = Self::default();
        for file in files.iter().filter(|file| is_python_source(file.path)) {
            index.files.insert(file.path.as_str().to_owned(), file.id);
            let mut dir = file.path.parent();
            // Every ancestor is recorded together with the first directory that is new, so a known directory
            // means its ancestors are known too.
            while index.dirs.insert(dir.to_owned()) {
                match dir.rsplit_once('/') {
                    Some((parent, _)) => dir = parent,
                    None if dir.is_empty() => break,
                    None => dir = "",
                }
            }
        }
        index
    }

    /// The module `segments` names under the directory `base`: `a/b.py`, else `a/b/__init__.py`, else `a/b.pyi`,
    /// else `a/b/__init__.pyi`, else a namespace package when the directory `a/b` holds Python files.
    fn spot(&self, base: &str, segments: &[&str]) -> Spot {
        let path = join(base, &segments.join("/"));
        let named = !segments.is_empty();
        let candidates = [
            named.then(|| format!("{path}.py")),
            Some(join(&path, "__init__.py")),
            named.then(|| format!("{path}.pyi")),
            Some(join(&path, "__init__.pyi")),
        ];
        let file = candidates
            .iter()
            .flatten()
            .find_map(|candidate| self.files.get(candidate.as_str()));
        match file {
            Some(file) => Spot::File(*file),
            None if self.dirs.contains(&path) => Spot::Namespace,
            None => Spot::Missing,
        }
    }
}

fn join(dir: &str, name: &str) -> String {
    match (dir.is_empty(), name.is_empty()) {
        (true, _) => name.to_owned(),
        (false, true) => dir.to_owned(),
        (false, false) => format!("{dir}/{name}"),
    }
}

/// Where `import` of the file `from` points.
pub(crate) fn resolve(project: &PythonProject, from: &RepoPath, import: &Import) -> Resolution {
    let spec = import.spec.as_str();
    let dots = spec.bytes().take_while(|byte| *byte == b'.').count();
    let module = spec.get(dots..).unwrap_or_default();
    let segments: Vec<&str> = module
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect();
    let index = &project.index;

    // A relative import starts from the importing file's package whatever the source roots are.
    if dots > 0 {
        let files =
            package_dir(from, dots).and_then(|base| reach(index, base, &segments, &import.names));
        return files.map_or(Resolution::Unresolved(UnresolvedReason::NotFound), found);
    }

    let mut first_party = false;
    for base in &project.source_roots {
        if let Some(files) = reach(index, base, &segments, &import.names) {
            return found(files);
        }
        let top = segments.first().map(std::slice::from_ref);
        first_party |= top.is_some_and(|top| index.spot(base, top) != Spot::Missing);
    }
    // The first segment is a package of the project and the rest of the path is not: a typo or a deleted
    // module, never a library of the same name.
    if first_party {
        return Resolution::Unresolved(UnresolvedReason::NotFound);
    }
    external(project, module)
}

/// The directory `level` dots start from: one dot is the package of the file, each further dot its parent.
fn package_dir(from: &RepoPath, level: usize) -> Option<&str> {
    let mut dir = from.parent();
    for _ in 1..level {
        if dir.is_empty() {
            return None;
        }
        dir = dir.rsplit_once('/').map_or("", |(parent, _)| parent);
    }
    Some(dir)
}

/// The files `from <module> import <names>` reaches under `base`, or `None` when nothing of it is there.
///
/// A name that is a submodule is imported as the module file; a name that is not is an attribute of the module,
/// so the module's own file is reached. When every name is a submodule the module's file is not.
fn reach(
    index: &ModuleIndex,
    base: &str,
    segments: &[&str],
    names: &[String],
) -> Option<Vec<FileId>> {
    let module = index.spot(base, segments);
    if module == Spot::Missing {
        return None;
    }
    // `*` imports what the module's file defines, never a submodule.
    let names: Vec<&String> = names.iter().filter(|name| name.as_str() != "*").collect();
    let mut files: Vec<FileId> = names
        .iter()
        .filter_map(|name| {
            let mut path = segments.to_vec();
            path.push(name.as_str());
            match index.spot(base, &path) {
                Spot::File(file) => Some(file),
                Spot::Namespace | Spot::Missing => None,
            }
        })
        .collect();
    let all_submodules = !names.is_empty() && files.len() == names.len();
    if !all_submodules && let Spot::File(file) = module {
        files.push(file);
    }
    files.sort_unstable();
    files.dedup();
    // A namespace package holds no file of its own. Importing it is fine; importing a name it does not have is not.
    if files.is_empty() && !names.is_empty() {
        return None;
    }
    Some(files)
}

fn found(files: Vec<FileId>) -> Resolution {
    match files.as_slice() {
        [file] => Resolution::File { file: *file },
        _ => Resolution::Files { files },
    }
}

/// A module outside the project: the standard library, a declared distribution, or a package nobody declared.
fn external(project: &PythonProject, module: &str) -> Resolution {
    let top = module.split('.').next().unwrap_or_default();
    if is_standard_library(top) {
        return Resolution::Builtin;
    }
    distributions_for_import(module)
        .into_iter()
        .find(|name| project.dependencies.contains(name))
        .map_or(
            Resolution::Unresolved(UnresolvedReason::UnknownPackage),
            |package| Resolution::External { package },
        )
}

/// Whether `top` is a standard-library module of some Python from 3.7 on. The project does not say which Python
/// it runs, so a module that any of them ships counts.
fn is_standard_library(top: &str) -> bool {
    (7..=15).any(|minor| is_known_standard_library(minor, top) || is_builtin_module(minor, top))
}
