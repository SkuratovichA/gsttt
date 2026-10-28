//! The module tree of each crate target, built from the `mod` declarations of its files.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, PoisonError, RwLock};

use gsttt_core::fact::{FileFacts, ImportKind};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;

use crate::manifest::{Target, TargetKind, at};

/// How many files deep `mod` declarations are followed. A `#[path]` that points back at an ancestor would otherwise
/// loop.
const MAX_DEPTH: usize = 64;

/// How many modules one crate target holds. Declarations that fan out through `#[path]`, several modules to a file
/// and every file declaring several more, would otherwise grow without bound. A tree that reaches it is cut, the cut
/// is logged, and every project names it among its blind spots.
pub(crate) const MAX_MODULES: usize = 10_000;

/// One module of a crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Module {
    /// The file the module is written in: the declaring file for an inline module.
    pub(crate) file: RepoPath,
    /// The directory in which `mod x;` declarations inside this module look for their files.
    dir: String,
    inline: bool,
}

impl Module {
    /// The module of a file that no `mod` declaration reaches, with its children where Cargo would look for them.
    pub(crate) fn orphan(file: &RepoPath) -> Self {
        let directory = file.parent();
        let dir = match file.file_name().strip_suffix(".rs") {
            Some("mod" | "lib" | "main") | None => directory.to_owned(),
            Some(stem) => at(directory, stem),
        };
        Self {
            file: file.clone(),
            dir,
            inline: false,
        }
    }

    /// The inline module `name` written in this one.
    pub(crate) fn inline_child(&self, name: &str) -> Self {
        Self {
            file: self.file.clone(),
            dir: at(&self.dir, name),
            inline: true,
        }
    }
}

/// A file a `mod` declaration names.
#[derive(Debug)]
pub(crate) struct Located {
    pub(crate) id: FileId,
    file: RepoPath,
    dir: String,
}

/// The file of the module `name` declared in `parent`: `<name>.rs` or `<name>/mod.rs` in the directory the parent
/// looks in, or the file a `#[path]` attribute names.
pub(crate) fn locate(
    repo: &RepoView,
    parent: &Module,
    name: &str,
    path_attribute: Option<&str>,
) -> Option<Located> {
    if let Some(attribute) = path_attribute {
        // An attribute on a declaration inside an inline module counts from that module's directory; otherwise it
        // counts from the directory of the declaring file.
        let base = if parent.inline {
            parent.dir.as_str()
        } else {
            parent.file.parent()
        };
        let file = RepoPath::join(base, attribute).ok()?;
        let id = repo.lookup(file.as_str())?;
        let dir = file.parent().to_owned();
        return Some(Located { id, file, dir });
    }
    [format!("{name}.rs"), format!("{name}/mod.rs")]
        .into_iter()
        .map(|relative| at(&parent.dir, &relative))
        .find_map(|candidate| {
            let id = repo.lookup(&candidate)?;
            Some(Located {
                id,
                file: repo.path(id)?.clone(),
                dir: at(&parent.dir, name),
            })
        })
}

/// The key of `name` inside the module `parent`: `a::b` for `b` inside `a`, `""` for the crate root.
fn child_key(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}::{name}")
    }
}

/// The key of the module around `key`, `None` for the crate root.
pub(crate) fn parent_key(key: &str) -> Option<&str> {
    if key.is_empty() {
        return None;
    }
    Some(key.rsplit_once("::").map_or("", |(parent, _)| parent))
}

/// The modules of one crate target, by key.
#[derive(Debug)]
pub(crate) struct CrateTree {
    kind: TargetKind,
    modules: BTreeMap<String, Module>,
    /// Whether declarations were left out at [`MAX_MODULES`].
    capped: bool,
}

impl CrateTree {
    /// `None` when the target's root is no repository path.
    fn build(target: &Target, repo: &RepoView, facts: &HashMap<&str, &FileFacts>) -> Option<Self> {
        let file = RepoPath::normalize(&target.root).ok()?;
        let root = Module {
            dir: file.parent().to_owned(),
            file: file.clone(),
            inline: false,
        };
        let mut tree = Self {
            kind: target.kind,
            modules: BTreeMap::from([(String::new(), root)]),
            capped: false,
        };
        let mut chain = vec![file];
        tree.expand("", repo, facts, &mut chain);
        if tree.capped {
            tracing::warn!(
                root = %target.root,
                cap = MAX_MODULES,
                "module tree cut at the module cap, so the modules past it are unknown"
            );
        }
        Some(tree)
    }

    /// Adds the modules declared in the file of the module at `key`, and then theirs. `chain` holds the files
    /// above, so a file that includes itself is entered once.
    fn expand(
        &mut self,
        key: &str,
        repo: &RepoView,
        facts: &HashMap<&str, &FileFacts>,
        chain: &mut Vec<RepoPath>,
    ) {
        let Some(module) = self.modules.get(key).cloned() else {
            return;
        };
        let Some(file_facts) = facts.get(module.file.as_str()) else {
            return;
        };
        for import in file_facts
            .imports
            .iter()
            .filter(|import| import.kind == ImportKind::Module)
        {
            let mut names: Vec<&str> = import.spec.split("::").collect();
            let Some(name) = names.pop() else {
                continue;
            };
            let mut parent_key = key.to_owned();
            let mut parent = module.clone();
            for inline in names {
                let inline_key = child_key(&parent_key, inline);
                parent = self
                    .modules
                    .entry(inline_key.clone())
                    .or_insert_with(|| parent.inline_child(inline))
                    .clone();
                parent_key = inline_key;
            }
            let attribute = import.names.first().map(String::as_str);
            let Some(found) = locate(repo, &parent, name, attribute) else {
                continue;
            };
            let Some(declared) = self.key_for(&parent_key, name, &found.file) else {
                continue;
            };
            if self.modules.len() >= MAX_MODULES {
                self.capped = true;
                return;
            }
            let descend = chain.len() < MAX_DEPTH && !chain.contains(&found.file);
            self.modules.insert(
                declared.clone(),
                Module {
                    file: found.file.clone(),
                    dir: found.dir,
                    inline: false,
                },
            );
            if descend {
                chain.push(found.file);
                self.expand(&declared, repo, facts, chain);
                chain.pop();
            }
        }
    }

    /// The key for the module `name` declared in the module at `parent` and written in `file`: its own name, or
    /// `name#2`, `name#3` and on when an earlier declaration of that name is written in another file, as one `mod`
    /// under different `cfg`s is. A copy is no name that code can write, so a path into the module reaches the first
    /// declaration, while the copy's file still has a module of its own to resolve `self::` and `super::` from.
    /// `None` when the module is already there.
    fn key_for(&self, parent: &str, name: &str, file: &RepoPath) -> Option<String> {
        let mut key = child_key(parent, name);
        let mut copies = 1_usize;
        while let Some(taken) = self.modules.get(&key) {
            if taken.file == *file {
                return None;
            }
            copies += 1;
            key = child_key(parent, &format!("{name}#{copies}"));
        }
        Some(key)
    }

    /// What kind of crate target the tree is of.
    pub(crate) fn kind(&self) -> TargetKind {
        self.kind
    }

    /// The module at `key`.
    pub(crate) fn module(&self, key: &str) -> Option<&Module> {
        self.modules.get(key)
    }

    /// Whether `name` is a module written directly in the module at `key`.
    pub(crate) fn has_child(&self, key: &str, name: &str) -> bool {
        self.modules.contains_key(&child_key(key, name))
    }

    /// The deepest module that following `names` from the module at `key` reaches. A name that is no module ends the
    /// walk: it is an item, a function or a type in the module before it.
    pub(crate) fn walk<'n>(
        &self,
        key: &str,
        names: impl IntoIterator<Item = &'n str>,
    ) -> Option<&Module> {
        let mut current = key.to_owned();
        for name in names {
            let next = child_key(&current, name);
            if !self.modules.contains_key(&next) {
                break;
            }
            current = next;
        }
        self.modules.get(&current)
    }
}

/// Where a file sits in its crate: the target and the key of its module.
#[derive(Debug)]
struct Home {
    target: usize,
    module: String,
}

/// The module trees of all crate targets of one package.
#[derive(Debug, Default)]
pub(crate) struct Trees {
    targets: Vec<CrateTree>,
    homes: HashMap<String, Home>,
}

impl Trees {
    /// Builds the tree of each target from the facts of the package's files. A file reached from several targets
    /// belongs to the first, and targets come library first.
    pub(crate) fn build(
        targets: &[Target],
        repo: &RepoView,
        facts: &HashMap<&str, &FileFacts>,
    ) -> Self {
        let targets: Vec<CrateTree> = targets
            .iter()
            .filter_map(|target| CrateTree::build(target, repo, facts))
            .collect();
        let mut homes = HashMap::new();
        for (target, tree) in targets.iter().enumerate() {
            for (key, module) in tree.modules.iter().filter(|(_, module)| !module.inline) {
                homes
                    .entry(module.file.as_str().to_owned())
                    .or_insert_with(|| Home {
                        target,
                        module: key.clone(),
                    });
            }
        }
        Self { targets, homes }
    }

    /// The tree a file belongs to and the key of its module in it. `None` for a file no `mod` declaration reaches.
    pub(crate) fn home(&self, file: &str) -> Option<(&CrateTree, &str)> {
        let home = self.homes.get(file)?;
        Some((self.targets.get(home.target)?, home.module.as_str()))
    }

    /// The tree of the package's library, which other crates and the package's other targets name.
    pub(crate) fn library(&self) -> Option<&CrateTree> {
        self.targets
            .iter()
            .find(|tree| tree.kind == TargetKind::Lib)
    }
}

/// The module trees of every package, by package directory, so that a path can continue into the library of a
/// workspace crate. Projects publish in `prepare` and read in `resolve`, and the engine finishes every `prepare`
/// before it starts any `resolve`.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    trees: RwLock<BTreeMap<String, Arc<Trees>>>,
}

impl Registry {
    pub(crate) fn publish(&self, dir: &str, trees: Arc<Trees>) {
        self.trees
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(dir.to_owned(), trees);
    }

    pub(crate) fn trees(&self, dir: &str) -> Option<Arc<Trees>> {
        self.trees
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
            .cloned()
    }
}
