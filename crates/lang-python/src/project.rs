//! The Python project: one per directory with a manifest, owning the Python files below it.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{
    ConnectorError, LinkInput, LinkedFact, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::fact::{Import, Resolution};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use xxhash_rust::xxh3::Xxh3;

use crate::PythonOptions;
use crate::extra::PyExtra;
use crate::files::{PROJECT_MANIFESTS, is_manifest, is_python_source, is_requirements, join_dir};
use crate::manifest::{
    Manifest, read_pyproject, read_requirements, read_setup_cfg, read_setup_py,
    requirement_includes,
};

/// A directory with a `pyproject.toml`, `setup.py` or `setup.cfg`, or the repository root when a Python file lies
/// under none of those directories. It knows its source roots, the distributions it declares and the packs they switch
/// on.
#[derive(Debug)]
pub struct PythonProject {
    pub(crate) root: String,
    /// The distribution the project's manifests name for it, normalised: what another project writes to depend on it.
    distribution: Option<String>,
    /// The directories modules are imported from, repository-relative, in the order they are tried.
    pub(crate) source_roots: Vec<String>,
    /// The declared distributions, normalised.
    pub(crate) dependencies: BTreeSet<String>,
    /// The packs in force: those written for Python whose `detect` list meets a declared distribution.
    pub(crate) packs: Vec<Pack>,
    salt: u64,
    /// The repository the project lives in, which resolution looks files up in.
    pub(crate) repo: RepoView,
    /// The source roots of the other Python projects of the repository that this project declares as dependencies and
    /// does not search itself, in the order of those projects' paths and each project's roots in the order it tries
    /// them: where a library of the repository is found. A project nobody declares is not on the path of an import, so
    /// what it holds is no module of this one.
    pub(crate) sibling_roots: Vec<String>,
    /// What a module in `sibling_roots` can be imported by, set with them: the directories and the file names, without
    /// `.py` and `.pyi`, directly in those roots. A name that is none of them is held by no sibling root.
    pub(crate) sibling_tops: BTreeSet<String>,
}

/// The projects of the repository, one per manifest directory, in path order.
pub(crate) fn open_projects(
    repo: &RepoView,
    options: &PythonOptions,
    packs: &[Pack],
) -> Result<Vec<PythonProject>, ConnectorError> {
    let mut roots: BTreeSet<String> = repo
        .files()
        .filter(|(_, path)| PROJECT_MANIFESTS.contains(&path.file_name()))
        .map(|(_, path)| path.parent().to_owned())
        .collect();
    // Python files without a manifest above them still need a project to be read; the repository root is it. Without
    // such a file the root would be a project with nothing to read.
    if !roots.contains("")
        && repo
            .files()
            .any(|(_, path)| is_python_source(path) && !is_held_by(&roots, path))
    {
        roots.insert(String::new());
    }
    // A requirements file belongs to the deepest project above it.
    let mut requirements: BTreeMap<&str, Vec<&RepoPath>> = BTreeMap::new();
    for (_, path) in repo.files().filter(|(_, path)| is_requirements(path)) {
        let owner = roots
            .iter()
            .filter(|root| path.is_under(root))
            .max_by_key(|root| root.len());
        if let Some(owner) = owner {
            requirements.entry(owner.as_str()).or_default().push(path);
        }
    }
    let mut projects = roots
        .iter()
        .map(|root| {
            let own = requirements
                .get(root.as_str())
                .map_or(&[][..], Vec::as_slice);
            open_project(repo, root, options, packs, own)
        })
        .collect::<Result<Vec<_>, _>>()?;
    // A project imports the libraries of the projects it declares, a dependency naming the distribution another
    // project's manifest gives it, so each learns where those look for modules.
    let mut named: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, project) in projects.iter().enumerate() {
        if let Some(name) = &project.distribution {
            named.entry(name).or_default().push(index);
        }
    }
    let siblings: Vec<Vec<String>> = projects
        .iter()
        .enumerate()
        .map(|(index, project)| {
            let declared: BTreeSet<usize> = project
                .dependencies
                .iter()
                .flat_map(|name| named.get(name.as_str()).into_iter().flatten().copied())
                .filter(|other| *other != index)
                .collect();
            let mut roots = Vec::new();
            for dir in declared.into_iter().flat_map(|other| {
                projects
                    .get(other)
                    .into_iter()
                    .flat_map(|project| &project.source_roots)
            }) {
                if !project.source_roots.contains(dir) {
                    push_unique(&mut roots, dir.clone());
                }
            }
            roots
        })
        .collect();
    // What a root can be imported by is read once, however many projects declare the project it belongs to.
    let mut tops: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for root in siblings.iter().flatten() {
        tops.entry(root.clone())
            .or_insert_with(|| top_names(repo, root));
    }
    for (project, roots) in projects.iter_mut().zip(siblings) {
        project.sibling_tops = roots
            .iter()
            .filter_map(|root| tops.get(root))
            .flatten()
            .cloned()
            .collect();
        project.sibling_roots = roots;
    }
    Ok(projects)
}

/// The names a module in the directory `root` can be imported by: the directories and the files directly in it, a
/// file without its `.py` or `.pyi`. A name that stands for no Python is in too, so the set is never smaller than the
/// names that resolve.
pub(crate) fn top_names(repo: &RepoView, root: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (_, path) in repo.under(root) {
        let below = if root.is_empty() {
            Some(path.as_str())
        } else {
            path.as_str()
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
        };
        let Some(below) = below else {
            continue;
        };
        let name = match below.split_once('/') {
            Some((directory, _)) => directory,
            None => below
                .strip_suffix(".pyi")
                .or_else(|| below.strip_suffix(".py"))
                .unwrap_or(below),
        };
        if !names.contains(name) {
            names.insert(name.to_owned());
        }
    }
    names
}

/// Whether `path` lies under one of the project `roots`: its directory or one above it is a root.
fn is_held_by(roots: &BTreeSet<String>, path: &RepoPath) -> bool {
    std::iter::successors(Some(path.parent()), |dir| {
        (!dir.is_empty()).then(|| dir.rsplit_once('/').map_or("", |(above, _)| above))
    })
    .any(|dir| roots.contains(dir))
}

fn open_project(
    repo: &RepoView,
    root: &str,
    options: &PythonOptions,
    packs: &[Pack],
    requirements: &[&RepoPath],
) -> Result<PythonProject, ConnectorError> {
    let manifest = read_manifests(repo, root, requirements);

    let mut source_roots = Vec::new();
    for declared in &options.source_roots {
        let dir = join_dir(root, declared).ok_or_else(|| ConnectorError::Project {
            connector: "python".to_owned(),
            root: root.to_owned(),
            message: format!(
                "source root `{declared}` leaves the repository; write it relative to the project's directory"
            ),
        })?;
        push_unique(&mut source_roots, dir);
    }
    for declared in &manifest.roots {
        if let Some(dir) = join_dir(root, declared) {
            push_unique(&mut source_roots, dir);
        } else {
            tracing::warn!(
                root,
                declared,
                "a package setting names a source root outside the repository; it is ignored"
            );
        }
    }
    if let Some(src) = join_dir(root, "src").filter(|src| repo.is_dir(src)) {
        push_unique(&mut source_roots, src);
    }
    push_unique(&mut source_roots, root.to_owned());

    let in_force: Vec<(usize, &Pack)> = packs
        .iter()
        .enumerate()
        .filter(|(_, pack)| {
            pack.language == "python"
                && pack.applies_to(manifest.dependencies.iter().map(String::as_str))
        })
        .collect();
    let salt = packs_salt(in_force.iter().map(|&(position, _)| position));
    let packs = in_force.into_iter().map(|(_, pack)| pack.clone()).collect();
    Ok(PythonProject {
        root: root.to_owned(),
        distribution: manifest.name,
        source_roots,
        dependencies: manifest.dependencies,
        packs,
        salt,
        repo: repo.clone(),
        sibling_roots: Vec::new(),
        sibling_tops: BTreeSet::new(),
    })
}

/// The salt of a project whose manifests switch on the packs at `positions` in the list the connector was given.
///
/// Parsing depends on those packs and on nothing else the project holds. The engine's key for a file already holds the
/// options and every pack the connector was given, in order, so what it cannot tell is which of them are in force
/// here, and the manifests decide that. A pack is named by its position because two packs may share a name.
fn packs_salt(positions: impl Iterator<Item = usize>) -> u64 {
    let positions: Vec<String> = positions.map(|position| position.to_string()).collect();
    let parts: Vec<&str> = positions.iter().map(String::as_str).collect();
    salt_of(&parts)
}

/// A hash of `parts` in which each part is taken with its length, so `["ab", "c"]` and `["a", "bc"]` are two salts.
/// The indexing change adds this function, with the same framing, to core as `gsttt_core::connector::salt_of`; this
/// copy goes when the two meet, and no salt changes with it.
fn salt_of(parts: &[&str]) -> u64 {
    let mut hasher = Xxh3::new();
    for part in parts {
        hasher.update(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.digest()
}

fn push_unique(roots: &mut Vec<String>, dir: String) {
    if !roots.contains(&dir) {
        roots.push(dir);
    }
}

/// What the project's own manifests and its requirements files declare. An unreadable manifest is logged and
/// skipped: the project still opens, only without what the manifest would have said.
fn read_manifests(repo: &RepoView, root: &str, requirements: &[&RepoPath]) -> Manifest {
    let mut manifest = Manifest::default();
    for name in PROJECT_MANIFESTS {
        let Some(path) = join_dir(root, name).filter(|path| repo.lookup(path).is_some()) else {
            continue;
        };
        let Some(text) = read_text(repo, &path) else {
            continue;
        };
        match name {
            "pyproject.toml" => match read_pyproject(&path, &text) {
                Ok(read) => manifest.absorb(read),
                Err(error) => tracing::warn!(%error, "a manifest is skipped"),
            },
            "setup.cfg" => manifest.absorb(read_setup_cfg(&text)),
            _ => manifest.absorb(read_setup_py(&text)),
        }
    }
    read_requirement_files(repo, requirements, &mut manifest);
    manifest
}

/// Reads the requirements `files` and the files they include with `-r`, each file once however many name it.
///
/// An include is relative to the file that names it, and a file that is not in the repository is skipped: one that
/// git ignores is not part of the tree the index describes.
fn read_requirement_files(repo: &RepoView, files: &[&RepoPath], manifest: &mut Manifest) {
    let mut pending: Vec<RepoPath> = files.iter().map(|&file| file.clone()).collect();
    let mut seen = BTreeSet::new();
    while let Some(file) = pending.pop() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let Some(text) = read_text(repo, file.as_str()) else {
            continue;
        };
        manifest.absorb(read_requirements(&text));
        for include in requirement_includes(&text) {
            match RepoPath::join(file.parent(), &include) {
                Ok(included) if repo.lookup(included.as_str()).is_some() => pending.push(included),
                _ => tracing::warn!(
                    file = file.as_str(),
                    include,
                    "an included requirements file is not in the repository and is skipped"
                ),
            }
        }
    }
}

fn read_text(repo: &RepoView, path: &str) -> Option<String> {
    match repo.read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            tracing::warn!(file = path, %error, "a manifest cannot be read and is skipped");
            None
        }
    }
}

impl Project for PythonProject {
    type Extra = PyExtra;

    fn root(&self) -> &str {
        &self.root
    }

    fn matches(&self, path: &RepoPath) -> bool {
        is_python_source(path) || is_manifest(path)
    }

    fn salt(&self) -> u64 {
        self.salt
    }

    fn provides(&self) -> Vec<Provide> {
        crate::provides::provides(&self.packs)
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<PyExtra> {
        crate::parse::parse(self, file)
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        crate::resolve::resolve(self, from, import)
    }

    fn link(&self, input: &LinkInput<'_, PyExtra>) -> Vec<LinkedFact> {
        crate::link::link(self, input)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_part_is_taken_with_its_length() {
        assert_ne!(salt_of(&["ab", "c"]), salt_of(&["a", "bc"]));
        assert_ne!(salt_of(&["a", ""]), salt_of(&["a"]));
        assert_ne!(salt_of(&[]), salt_of(&[""]));
        assert_eq!(salt_of(&["0", "2"]), salt_of(&["0", "2"]));
    }

    #[test]
    fn the_packs_in_force_are_told_by_position() {
        assert_ne!(packs_salt([0].into_iter()), packs_salt([1].into_iter()));
        assert_ne!(packs_salt([1, 2].into_iter()), packs_salt([12].into_iter()));
        assert_eq!(
            packs_salt([3, 5].into_iter()),
            packs_salt([3, 5].into_iter())
        );
    }

    proptest! {
        #[test]
        fn moving_the_boundary_between_parts_changes_the_salt(
            first in "\\PC{0,12}",
            middle in "\\PC{1,12}",
            last in "\\PC{0,12}",
        ) {
            let left = format!("{first}{middle}");
            let right = format!("{middle}{last}");
            prop_assert_ne!(salt_of(&[&left, &last]), salt_of(&[&first, &right]));
        }
    }
}
