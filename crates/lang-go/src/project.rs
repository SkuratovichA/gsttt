//! The Go project: one per `go.mod`, owning the Go files below it that no deeper `go.mod` owns.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use gsttt_core::connector::{LinkInput, Linked, Parsed, Project, Provide, SourceFile, salt_of};
use gsttt_core::fact::{Import, Resolution};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;

use crate::files::{MANIFEST, is_go_source, is_manifest};
use crate::interfaces::GoExtra;
use crate::manifest::read_go_mod;

/// Bumped when what the reader makes of a file changes without a change to the facts' shape, so that stored facts are read again.
const READER_VERSION: &str = "5";

/// One file of a package, as `prepare` keeps it to resolve imports of the package.
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) file: FileId,
    /// The name in the package clause, `-` for a file left out of every build, `None` when the file was read past errors.
    pub(crate) package: Option<String>,
    pub(crate) test: bool,
    /// The heads of the names the file declares: `T` for `T` and for `T.M`.
    pub(crate) heads: BTreeSet<String>,
    /// Whether the file runs something as the package loads.
    pub(crate) on_load: bool,
    /// Whether the file could not be read past a syntax error, so it may declare anything.
    pub(crate) opaque: bool,
}

/// A directory with a `go.mod`.
#[derive(Debug)]
pub struct GoProject {
    pub(crate) root: String,
    /// The modules the `go.mod` requires, with the ones marked indirect.
    pub(crate) requires: BTreeSet<String>,
    /// The module path and the directory of every `go.mod` of the repository, the longest path first.
    pub(crate) repo_modules: Vec<(String, String)>,
    /// The module paths alone, for the reader of a file.
    pub(crate) modules: Vec<String>,
    /// The packs in force: those written for Go whose `detect` list meets a required module.
    pub(crate) packs: Vec<Pack>,
    salt: u64,
    pub(crate) repo: RepoView,
    /// The files of each directory of the project, set by `prepare`.
    pub(crate) plan: Arc<Plan>,
    /// The plan of every project by its root, shared so that an import of a package of another module reads what that module's `prepare` noted.
    pub(crate) plans: Arc<Mutex<BTreeMap<String, Arc<Plan>>>>,
}

/// The files of the directories of one project.
pub(crate) type Plan = BTreeMap<String, Vec<Entry>>;

/// The projects of the repository, one per `go.mod` outside `vendor`, `testdata` and the like, in path order.
pub(crate) fn open_projects(repo: &RepoView, packs: &[Pack]) -> Vec<GoProject> {
    let manifests: Vec<(String, String, BTreeSet<String>)> = repo
        .files()
        .filter(|(_, path)| is_manifest(path))
        .filter_map(|(_, path)| {
            let text = repo.read_regular_file(path.as_str())?;
            let read = read_go_mod(&text);
            Some((path.parent().to_owned(), read.module, read.requires))
        })
        .collect();
    let mut repo_modules: Vec<(String, String)> = manifests
        .iter()
        .filter(|(_, module, _)| !module.is_empty())
        .map(|(dir, module, _)| (module.clone(), dir.clone()))
        .collect();
    repo_modules.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
    let modules: Vec<String> = {
        let mut all: Vec<String> = repo_modules
            .iter()
            .map(|(module, _)| module.clone())
            .collect();
        all.sort();
        all.dedup();
        all
    };
    let plans: Arc<Mutex<BTreeMap<String, Arc<Plan>>>> = Arc::default();
    let mut projects = Vec::with_capacity(manifests.len());
    for (dir, module, requires) in manifests {
        if module.is_empty() {
            tracing::warn!(
                file = RepoPath::join_dir(&dir, MANIFEST).unwrap_or_else(|| MANIFEST.to_owned()),
                "a go.mod without a `module` line has an empty module path; no import is read as belonging to it"
            );
        }
        let in_force: Vec<(usize, &Pack)> = packs
            .iter()
            .enumerate()
            .filter(|(_, pack)| {
                pack.language == "go" && pack.applies_to(requires.iter().map(String::as_str))
            })
            .collect();
        let positions: Vec<String> = in_force.iter().map(|(at, _)| at.to_string()).collect();
        let mut parts: Vec<&str> = vec![READER_VERSION];
        parts.extend(modules.iter().map(String::as_str));
        parts.push("packs");
        parts.extend(positions.iter().map(String::as_str));
        projects.push(GoProject {
            root: dir,
            requires,
            repo_modules: repo_modules.clone(),
            modules: modules.clone(),
            packs: in_force.into_iter().map(|(_, pack)| pack.clone()).collect(),
            salt: salt_of(&parts),
            repo: repo.clone(),
            plan: Arc::default(),
            plans: Arc::clone(&plans),
        });
    }
    projects
}

impl Project for GoProject {
    type Extra = GoExtra;

    fn root(&self) -> &str {
        &self.root
    }

    fn matches(&self, path: &RepoPath) -> bool {
        is_go_source(path) || is_manifest(path)
    }

    fn salt(&self) -> u64 {
        self.salt
    }

    fn provides(&self) -> Vec<Provide> {
        crate::provides::provides(&self.packs)
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<GoExtra> {
        crate::parse::parse(self, file)
    }

    fn code(&self, file: &SourceFile<'_>) -> Option<String> {
        crate::code::blanked(file)
    }

    fn prepare(&mut self, files: &[gsttt_core::connector::PrepareFile<'_>]) {
        crate::resolve::prepare(self, files);
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        crate::resolve::resolve(self, from, import)
    }

    fn link(&self, input: &LinkInput<'_, GoExtra>) -> Linked {
        Linked {
            overrides: crate::interfaces::link(input),
            ..Linked::default()
        }
    }
}
