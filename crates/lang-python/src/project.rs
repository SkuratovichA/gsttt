//! The Python project: one per directory with a manifest, owning the Python files below it.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{
    ConnectorError, LinkInput, LinkedFact, Parsed, PrepareFile, Project, Provide, SourceFile,
};
use gsttt_core::fact::{Import, Resolution};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use xxhash_rust::xxh3::xxh3_64;

use crate::PythonOptions;
use crate::extra::PyExtra;
use crate::files::{PROJECT_MANIFESTS, is_manifest, is_python_source, is_requirements, join_dir};
use crate::manifest::{Manifest, read_pyproject, read_requirements, read_setup_cfg, read_setup_py};
use crate::resolve::ModuleIndex;

/// A directory with a `pyproject.toml`, `setup.py` or `setup.cfg`, or the repository root when it has Python
/// files and none of these. It knows its source roots, the distributions it declares and the packs they switch on.
#[derive(Debug)]
pub struct PythonProject {
    pub(crate) root: String,
    /// The directories modules are imported from, repository-relative, in the order they are tried.
    pub(crate) source_roots: Vec<String>,
    /// The declared distributions, normalised.
    pub(crate) dependencies: BTreeSet<String>,
    /// The packs in force: those written for Python whose `detect` list meets a declared distribution.
    pub(crate) packs: Vec<Pack>,
    salt: u64,
    /// The project's Python files by path, built in `prepare`.
    pub(crate) index: ModuleIndex,
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
    // Python files without a manifest above them still need a project to be read; the repository root is it.
    if !roots.contains("") && repo.files().any(|(_, path)| is_python_source(path)) {
        roots.insert(String::new());
    }
    // A requirements file belongs to the deepest project above it.
    let mut requirements: BTreeMap<&str, Vec<&RepoPath>> = BTreeMap::new();
    for (_, path) in repo
        .files()
        .filter(|(_, path)| is_requirements(path.file_name()))
    {
        let owner = roots
            .iter()
            .filter(|root| path.is_under(root))
            .max_by_key(|root| root.len());
        if let Some(owner) = owner {
            requirements.entry(owner.as_str()).or_default().push(path);
        }
    }
    roots
        .iter()
        .map(|root| {
            let own = requirements
                .get(root.as_str())
                .map_or(&[][..], Vec::as_slice);
            open_project(repo, root, options, packs, own)
        })
        .collect()
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

    let packs: Vec<Pack> = packs
        .iter()
        .filter(|pack| {
            pack.language == "python"
                && pack.applies_to(manifest.dependencies.iter().map(String::as_str))
        })
        .cloned()
        .collect();
    // Parsing depends on the packs in force and on nothing else the project holds.
    let salt = xxh3_64(serde_json::to_string(&packs).unwrap_or_default().as_bytes());
    Ok(PythonProject {
        root: root.to_owned(),
        source_roots,
        dependencies: manifest.dependencies,
        packs,
        salt,
        index: ModuleIndex::default(),
    })
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
    for path in requirements {
        if let Some(text) = read_text(repo, path.as_str()) {
            manifest.absorb(read_requirements(&text));
        }
    }
    manifest
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
        is_python_source(path) || is_manifest(path.file_name())
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

    fn prepare(&mut self, files: &[PrepareFile<'_>]) {
        self.index = ModuleIndex::build(files);
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        crate::resolve::resolve(self, from, import)
    }

    fn link(&self, input: &LinkInput<'_, PyExtra>) -> Vec<LinkedFact> {
        crate::link::link(input)
    }
}
