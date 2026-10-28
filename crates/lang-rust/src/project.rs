//! One Rust project: a `Cargo.toml` and the files below it that no deeper manifest owns.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;

use gsttt_core::connector::{
    LinkInput, LinkedFact, Parsed, PrepareFile, Project, Provide, SourceFile,
};
use gsttt_core::fact::{FileFacts, FileTags, Import, ImportKind, ParseStatus, Resolution};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use gsttt_core::text::LineIndex;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast;
use xxhash_rust::xxh3::xxh3_64;

use crate::imports;
use crate::link;
use crate::manifest::Package;
use crate::provides;
use crate::resolve::{Aliases, Resolver};
use crate::sites;
use crate::syntax::line_at;
use crate::tree::{Registry, Trees};

/// The language name packs use to say they are for this connector.
const LANGUAGE: &str = "rust";

/// A Rust package, or the manifest of a workspace that has none.
#[derive(Debug)]
pub struct RustProject {
    root: String,
    package: Option<Package>,
    edition: Edition,
    /// The packs the manifest switches on.
    packs: Vec<Pack>,
    repo: RepoView,
    /// Shared by every project of the repository, so a path can continue into a workspace crate.
    registry: Arc<Registry>,
    trees: Arc<Trees>,
    aliases: Aliases,
    salt: u64,
}

impl RustProject {
    /// The project of the manifest in `root`. `default_edition` applies when the package names none.
    pub(crate) fn new(
        root: &str,
        package: Option<Package>,
        default_edition: Edition,
        packs: &[Pack],
        repo: &RepoView,
        registry: Arc<Registry>,
    ) -> Self {
        let edition = package
            .as_ref()
            .and_then(|package| package.edition.as_deref())
            .and_then(|text| {
                Edition::from_str(text)
                    .inspect_err(|error| {
                        tracing::warn!(root, %error, "unknown edition in the manifest, parsing with the default");
                    })
                    .ok()
            })
            .unwrap_or(default_edition);
        let dependencies: Vec<&str> = package
            .iter()
            .flat_map(|package| &package.dependencies)
            .flat_map(|dependency| [dependency.package.as_str(), dependency.key.as_str()])
            .collect();
        let packs: Vec<Pack> = packs
            .iter()
            .filter(|pack| {
                pack.language == LANGUAGE && pack.applies_to(dependencies.iter().copied())
            })
            .cloned()
            .collect();
        // Anything but the file text that changes what `parse` returns: the edition decides the syntax, the
        // packs decide the contracts.
        let packs_json = serde_json::to_string(&packs).unwrap_or_default();
        let salt = xxh3_64(format!("{edition};{packs_json}").as_bytes());
        Self {
            root: root.to_owned(),
            package,
            edition,
            packs,
            repo: repo.clone(),
            registry,
            trees: Arc::new(Trees::default()),
            aliases: Aliases::default(),
            salt,
        }
    }

    /// Whether the `sqlx` pack is in force, whose `query_file!` macros read a file named relative to the manifest.
    fn reads_query_files(&self) -> bool {
        self.packs.iter().any(|pack| pack.name == "sqlx")
    }

    /// Whether `path` is a test file by its place in the package: under a `tests` directory, or named `tests`, `test`,
    /// `*_test` or `*_tests`.
    fn is_test(&self, path: &RepoPath) -> bool {
        let relative = path
            .as_str()
            .strip_prefix(self.root.as_str())
            .map_or(path.as_str(), |rest| rest.trim_start_matches('/'));
        let (directories, name) = relative.rsplit_once('/').unwrap_or(("", relative));
        let stem = name.strip_suffix(".rs").unwrap_or(name);
        directories.split('/').any(|directory| directory == "tests")
            || matches!(stem, "tests" | "test")
            || stem.ends_with("_test")
            || stem.ends_with("_tests")
    }
}

impl Project for RustProject {
    type Extra = link::Extra;

    fn root(&self) -> &str {
        &self.root
    }

    fn matches(&self, path: &RepoPath) -> bool {
        path.extension() == Some("rs") || path.file_name() == "Cargo.toml"
    }

    fn salt(&self) -> u64 {
        self.salt
    }

    fn provides(&self) -> Vec<Provide> {
        provides::provides(&self.packs)
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<link::Extra> {
        if file.path.file_name() == "Cargo.toml" {
            return Parsed::new(FileFacts {
                tags: FileTags::MANIFEST,
                ..FileFacts::default()
            });
        }
        let parse = ast::SourceFile::parse(file.text, self.edition);
        let lines = LineIndex::new(file.text);
        let errors = parse.errors();
        let status = match errors.iter().map(|error| error.range().start()).min() {
            Some(first) => ParseStatus::Recovered {
                errors: u32::try_from(errors.len()).unwrap_or(u32::MAX),
                first_error_line: line_at(&lines, first),
            },
            None => ParseStatus::Ok,
        };
        let root = parse.syntax_node();
        let sites = sites::read(&root, &lines, &self.packs, self.edition);
        Parsed {
            facts: FileFacts {
                status,
                tags: if self.is_test(file.path) {
                    FileTags::TEST
                } else {
                    FileTags::NONE
                },
                imports: imports::read(&root, &lines, self.reads_query_files(), self.edition),
                contracts: sites.found.into_iter().map(|found| found.fact).collect(),
                embedded: sites.embedded,
                unresolved: sites.unresolved,
            },
            extra: sites.extra,
        }
    }

    fn prepare(&mut self, files: &[PrepareFile<'_>]) {
        let facts: HashMap<&str, &FileFacts> = files
            .iter()
            .map(|file| (file.path.as_str(), file.facts))
            .collect();
        let targets = self
            .package
            .as_ref()
            .map_or(&[][..], |package| package.targets.as_slice());
        self.trees = Arc::new(Trees::build(targets, &self.repo, &facts));
        self.aliases = aliases(files);
        if self.package.is_some() {
            self.registry.publish(&self.root, Arc::clone(&self.trees));
        }
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        Resolver {
            repo: &self.repo,
            root: &self.root,
            package: self.package.as_ref(),
            trees: &self.trees,
            registry: &self.registry,
            aliases: &self.aliases,
            edition: self.edition,
        }
        .resolve(from, import)
    }

    fn link(&self, input: &LinkInput<'_, link::Extra>) -> Vec<LinkedFact> {
        link::link(input, &|from, spec| {
            self.resolve(from, &Import::new(spec, ImportKind::Static, 1))
        })
    }
}

/// The names each file brings in with `use` or `extern crate`, each with the path it stands for. The first `use` of
/// a name wins.
fn aliases(files: &[PrepareFile<'_>]) -> Aliases {
    files
        .iter()
        .filter_map(|file| {
            let mut names = BTreeMap::new();
            for import in
                file.facts.imports.iter().filter(|import| {
                    matches!(import.kind, ImportKind::Static | ImportKind::ReExport)
                })
            {
                if let [name] = import.names.as_slice()
                    && name != "*"
                {
                    names
                        .entry(name.clone())
                        .or_insert_with(|| import.spec.clone());
                }
            }
            (!names.is_empty()).then(|| (file.path.as_str().to_owned(), names))
        })
        .collect()
}
