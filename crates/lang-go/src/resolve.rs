//! Where an import of a Go file leads.

use std::collections::BTreeSet;
use std::sync::{Arc, PoisonError};

use gsttt_core::connector::PrepareFile;
use gsttt_core::fact::{FileTags, Import, ImportKind, Resolution, UnresolvedReason};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;

use crate::file::is_under;
use crate::files::{is_go_source, is_test_name};
use crate::project::{Entry, GoProject, Plan};

/// Notes, per directory, which files declare what, so that an import of a package reaches the files that declare what the importer reads.
pub(crate) fn prepare(project: &mut GoProject, files: &[PrepareFile<'_>]) {
    let mut plan = Plan::new();
    for file in files {
        if file.facts.tags.contains(FileTags::MANIFEST) {
            continue;
        }
        let (heads, on_load, opaque) = match &file.facts.symbols {
            Some(symbols) => (
                symbols
                    .declared
                    .iter()
                    .map(|declared| head(&declared.name).to_owned())
                    .collect::<BTreeSet<_>>(),
                !symbols.on_load.is_empty(),
                false,
            ),
            None => (BTreeSet::new(), false, true),
        };
        plan.entry(file.path.parent().to_owned())
            .or_default()
            .push(Entry {
                file: file.id,
                package: file.facts.package.clone(),
                test: file.facts.tags.contains(FileTags::TEST),
                heads,
                on_load,
                opaque,
            });
    }
    let plan = Arc::new(plan);
    project.plan = Arc::clone(&plan);
    project
        .plans
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(project.root.clone(), plan);
}

/// `T` for `T` and for `T.M`.
fn head(name: &str) -> &str {
    name.split_once('.').map_or(name, |(head, _)| head)
}

pub(crate) fn resolve(project: &GoProject, from: &RepoPath, import: &Import) -> Resolution {
    let spec = import.spec.as_str();
    if spec == "." {
        return same_package(project, from, import);
    }
    if spec == "C" {
        return Resolution::Builtin;
    }
    if let Some((module, root)) = project
        .repo_modules
        .iter()
        .find(|(module, _)| is_under(spec, module))
    {
        let below = spec
            .get(module.len()..)
            .unwrap_or_default()
            .trim_start_matches('/');
        let Some(dir) = RepoPath::join_dir(root, below) else {
            return Resolution::Unresolved(UnresolvedReason::NotFound);
        };
        return if *root == project.root {
            in_module(project, &dir, import)
        } else {
            other_module(project, root, &dir, import)
        };
    }
    if let Some(module) = project
        .requires
        .iter()
        .filter(|module| is_under(spec, module))
        .max_by_key(|module| module.len())
    {
        return Resolution::External {
            package: module.clone(),
        };
    }
    let mut segments = spec.split('/');
    let first = segments.next().unwrap_or_default();
    if !first.contains('.') {
        return Resolution::Builtin;
    }
    // A host and an owner and a name make a module path, so the first three segments stand for the module the package belongs to.
    let package: Vec<&str> = spec.split('/').take(3).collect();
    if package.len() < 2 {
        return Resolution::Unresolved(UnresolvedReason::UnknownPackage);
    }
    Resolution::External {
        package: package.join("/"),
    }
}

/// The files of the package of `from` that declare a name the file reads. A file of another package of the directory, such as an external test package or a generator that `//go:build ignore` leaves out, is no sibling; one read past errors has no known package and counts.
fn same_package(project: &GoProject, from: &RepoPath, import: &Import) -> Resolution {
    let Some(entries) = project.plan.get(from.parent()) else {
        return Resolution::Files { files: Vec::new() };
    };
    let from_id = project.repo.lookup(from.as_str());
    let Some(me) = entries.iter().find(|entry| Some(entry.file) == from_id) else {
        return Resolution::Files { files: Vec::new() };
    };
    let files = entries
        .iter()
        .filter(|entry| Some(entry.file) != from_id)
        .filter(|entry| me.test || !entry.test)
        .filter(|entry| match (&me.package, &entry.package) {
            (Some(mine), Some(theirs)) => mine == theirs,
            _ => true,
        })
        .filter(|entry| declares(entry, &import.names))
        .map(|entry| entry.file)
        .collect();
    Resolution::Files { files }
}

fn declares(entry: &Entry, names: &[String]) -> bool {
    entry.opaque || names.iter().any(|name| entry.heads.contains(name))
}

/// A package of the project itself: the files that declare what the importer reads, and the files that run as it loads.
fn in_module(project: &GoProject, dir: &str, import: &Import) -> Resolution {
    match project.plan.get(dir) {
        Some(entries) => narrowed(entries, import),
        None => whole_dir(project, dir),
    }
}

/// A package of another module of the repository, narrowed by what that module's own `prepare` noted. When it noted nothing of the directory, the directory is read whole.
fn other_module(project: &GoProject, root: &str, dir: &str, import: &Import) -> Resolution {
    let plan = project
        .plans
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(root)
        .cloned();
    match plan.as_deref().and_then(|plan| plan.get(dir)) {
        Some(entries) => narrowed(entries, import),
        None => whole_dir(project, dir),
    }
}

/// The files of a package that an import reaches: all of them for an import that reads the package whole or finds no file declaring what it reads, else those that declare it and those that run as the package loads.
fn narrowed(entries: &[Entry], import: &Import) -> Resolution {
    let plain: Vec<&Entry> = entries.iter().filter(|entry| !entry.test).collect();
    let reads_whole = import.kind == ImportKind::SideEffect || import.names.is_empty();
    let declaring: Vec<&Entry> = plain
        .iter()
        .copied()
        .filter(|entry| declares(entry, &import.names))
        .collect();
    let chosen: Vec<FileId> = if reads_whole || declaring.is_empty() {
        plain.iter().map(|entry| entry.file).collect()
    } else {
        plain
            .iter()
            .filter(|entry| entry.on_load || declaring.iter().any(|d| d.file == entry.file))
            .map(|entry| entry.file)
            .collect()
    };
    Resolution::Files { files: chosen }
}

/// Every non-test file of the directory, for a package nothing was prepared for.
fn whole_dir(project: &GoProject, dir: &str) -> Resolution {
    let go: Vec<(FileId, &RepoPath)> = project
        .repo
        .dir_files(dir)
        .filter(|(_, path)| is_go_source(path))
        .collect();
    if go.is_empty() {
        return Resolution::Unresolved(UnresolvedReason::NotFound);
    }
    Resolution::Files {
        files: go
            .into_iter()
            .filter(|(_, path)| !is_test_name(path.file_name()))
            .map(|(id, _)| id)
            .collect(),
    }
}
