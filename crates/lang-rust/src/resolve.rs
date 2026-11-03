//! Where an import of a Rust file points: through the module tree of its crate, the library of its package, its dependencies, and the workspace's other crates.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use gsttt_core::fact::{Import, ImportKind, Resolution, UnresolvedReason};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use ra_ap_edition::Edition;

use crate::manifest::{Dependency, Package, TargetKind};
use crate::tree::{CrateTree, Module, Registry, Trees, locate, parent_key};

/// Crates every Rust program can name without declaring them.
const BUILTINS: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

/// How many times a path goes on through a re-export before the resolver stops at the module it reached, so that two crates that glob each other end.
const MAX_FORWARDS: u8 = 8;

/// What the files of a project bring in with `use`.
#[derive(Debug, Default)]
pub(crate) struct Aliases {
    /// The names, by file, each with the path it stands for.
    pub(crate) names: HashMap<String, BTreeMap<String, String>>,
    /// The paths of the glob imports, by file, in the order of the file.
    pub(crate) globs: HashMap<String, Vec<String>>,
}

/// What resolving needs, borrowed from the project that asks.
#[derive(Clone, Copy)]
pub(crate) struct Resolver<'a> {
    pub(crate) repo: &'a RepoView,
    pub(crate) root: &'a str,
    pub(crate) package: Option<&'a Package>,
    pub(crate) trees: &'a Trees,
    pub(crate) registry: &'a Registry,
    pub(crate) aliases: &'a Aliases,
    /// The edition of the crate, which decides where a `use` path starts.
    pub(crate) edition: Edition,
    /// How many re-exports the path has gone on through.
    pub(crate) forwards: u8,
    /// The files and the paths left that one import has gone on from, because modules that glob each other lead back to them with nothing new.
    pub(crate) seen: &'a RefCell<BTreeSet<(String, String)>>,
}

fn not_found() -> Resolution {
    Resolution::Unresolved(UnresolvedReason::NotFound)
}

impl Resolver<'_> {
    pub(crate) fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        match import.kind {
            ImportKind::Module => self.module(from, import),
            ImportKind::Include => self.include(from, &import.spec),
            _ => self.path(from.as_str(), &import.spec),
        }
    }

    /// The module a file declares with `mod`, found the way the compiler finds it.
    fn module(&self, from: &RepoPath, import: &Import) -> Resolution {
        let mut names: Vec<&str> = import.spec.split("::").collect();
        let Some(name) = names.pop() else {
            return not_found();
        };
        let mut parent = self.file_module(from);
        for inline in names {
            parent = parent.inline_child(inline);
        }
        let attribute = import.names.first().map(String::as_str);
        locate(self.repo, &parent, name, attribute)
            .map_or_else(not_found, |found| Resolution::File { file: found.id })
    }

    /// The module a file is, in the tree that reaches it, or where Cargo would put it when none does.
    fn file_module(&self, file: &RepoPath) -> Module {
        self.trees
            .home(file.as_str())
            .and_then(|(tree, key)| tree.module(key))
            .cloned()
            .unwrap_or_else(|| Module::orphan(file))
    }

    /// A file read at compile time. A literal path counts from the directory of the including file; one that starts at `CARGO_MANIFEST_DIR` counts from the package; one that starts at `OUT_DIR` is written by a build script.
    fn include(&self, from: &RepoPath, spec: &str) -> Resolution {
        let target = if let Some(rest) = spec.strip_prefix("${CARGO_MANIFEST_DIR}/") {
            RepoPath::join(self.root, rest)
        } else if spec.starts_with("${OUT_DIR}") {
            return Resolution::Unresolved(UnresolvedReason::Generated);
        } else if spec.contains("${") {
            return Resolution::Unresolved(UnresolvedReason::NonLiteral);
        } else {
            RepoPath::join(from.parent(), spec)
        };
        target
            .ok()
            .and_then(|path| self.repo.lookup(path.as_str()))
            .map_or_else(not_found, |file| Resolution::File { file })
    }

    /// A path of `use` or of code. A first segment the file brought in with `use` is replaced by what it stands for, once each, so two uses that name each other cannot loop.
    fn path(&self, from: &str, spec: &str) -> Resolution {
        let mut segments: Vec<String> = spec.split("::").map(str::to_owned).collect();
        let mut applied: Vec<String> = Vec::new();
        while let Some(first) = segments.first()
            && !matches!(first.as_str(), "crate" | "self" | "super")
            && !applied.contains(first)
            // The `use` that is being resolved defines its own name; replacing that name by the path would repeat it.
            && let Some(target) = self
                .aliases
                .names
                .get(from)
                .and_then(|names| names.get(first))
                .filter(|target| target.as_str() != spec)
        {
            applied.push(first.clone());
            let mut expanded: Vec<String> = target.split("::").map(str::to_owned).collect();
            expanded.extend(segments.drain(1..));
            segments = expanded;
        }
        let names: Vec<&str> = segments.iter().map(String::as_str).collect();
        let home = self.trees.home(from);
        match names.first().copied() {
            Some("") => self.rooted(home, names.get(1..).unwrap_or_default()),
            Some("crate") => home.map_or_else(not_found, |(tree, _)| {
                self.land(tree, "", names.get(1..).unwrap_or_default())
            }),
            Some("self") => home.map_or_else(not_found, |(tree, key)| {
                self.land(tree, key, names.get(1..).unwrap_or_default())
            }),
            Some("super") => self.above(home, &names),
            Some(_) => self.named(from, home, &names),
            None => not_found(),
        }
    }

    /// `super::super::x`: climbs one module per `super`, then walks the rest.
    fn above(&self, home: Option<(&CrateTree, &str)>, names: &[&str]) -> Resolution {
        let Some((tree, mut key)) = home else {
            return not_found();
        };
        let climbs = names.iter().take_while(|name| **name == "super").count();
        for _ in 0..climbs {
            let Some(parent) = parent_key(key) else {
                return not_found();
            };
            key = parent;
        }
        self.land(tree, key, names.get(climbs..).unwrap_or_default())
    }

    /// A path whose first segment names a child module, a crate, or a module at the crate root, in that order.
    ///
    /// The last step is not Rust's rule, which starts a path at the module or at a crate. It stands in for the names that `use super::*` and `use crate::…::*` bring in, which the connector does not read: a first segment that is a module at the crate root is more likely one of those than a mistake.
    fn named(&self, from: &str, home: Option<(&CrateTree, &str)>, names: &[&str]) -> Resolution {
        let Some((first, rest)) = names.split_first() else {
            return not_found();
        };
        if let Some((tree, key)) = home
            && tree.has_child(key, first)
        {
            return self.land(tree, key, names);
        }
        if let Some(resolution) = self.krate(home, first, rest) {
            return resolution;
        }
        if let Some((tree, _)) = home
            && tree.has_child("", first)
        {
            return self.land(tree, "", names);
        }
        // `use counting::Foo` with `counting` brought in by a glob of the same file.
        self.forwarded(from, names)
            .unwrap_or(Resolution::Unresolved(UnresolvedReason::UnknownPackage))
    }

    /// `::name::…`: the crate root in 2015, where `use` paths start; a crate from 2018 on, so a module of this crate is not one.
    fn rooted(&self, home: Option<(&CrateTree, &str)>, names: &[&str]) -> Resolution {
        let Some((first, rest)) = names.split_first() else {
            return not_found();
        };
        if self.edition == Edition::Edition2015
            && let Some((tree, _)) = home
            && tree.has_child("", first)
        {
            return self.land(tree, "", names);
        }
        self.krate(home, first, rest)
            .unwrap_or(Resolution::Unresolved(UnresolvedReason::UnknownPackage))
    }

    /// A crate named by its first segment: the package's own library, a declared dependency, or a builtin. A build script sees the build dependencies and nothing else, and no other target sees those.
    fn krate(
        &self,
        home: Option<(&CrateTree, &str)>,
        name: &str,
        rest: &[&str],
    ) -> Option<Resolution> {
        let kind = home.map_or(TargetKind::Lib, |(tree, _)| tree.kind());
        if let Some(package) = self.package {
            if kind != TargetKind::Build
                && package.lib_name == name
                && let Some(library) = self.trees.library()
            {
                return Some(self.land(library, "", rest));
            }
            if let Some(dependency) = package
                .dependencies
                .iter()
                .find(|dependency| dependency.visible_to(kind) && dependency.is_named(name))
            {
                return Some(self.dependency(dependency, rest));
            }
        }
        BUILTINS.contains(&name).then_some(Resolution::Builtin)
    }

    /// A dependency: the library of a workspace crate when it is one, else the package it names.
    fn dependency(&self, dependency: &Dependency, rest: &[&str]) -> Resolution {
        let published = dependency
            .path
            .as_deref()
            .and_then(|dir| Some((dir, self.registry.crate_at(dir)?)));
        let found = published
            .as_ref()
            .and_then(|(dir, published)| Some((*dir, published, published.trees.library()?)));
        let Some((dir, published, library)) = found else {
            return Resolution::External {
                package: dependency.package.clone(),
            };
        };
        // The rest of the path is the dependency's own: it goes on through what that crate re-exports, with the dependencies of that crate.
        Resolver {
            root: dir,
            package: published.package.as_ref(),
            trees: &published.trees,
            aliases: &published.aliases,
            edition: published.edition,
            ..*self
        }
        .land(library, "", rest)
    }

    /// The file of the deepest module that `names` reach from the module at `key`, or of what the path goes on through from there.
    fn land(&self, tree: &CrateTree, key: &str, names: &[&str]) -> Resolution {
        let Some((module, rest)) = tree.reach(key, names) else {
            return not_found();
        };
        if let Some(found) = self.forwarded(module.file.as_str(), rest) {
            return found;
        }
        self.repo
            .lookup(module.file.as_str())
            .map_or_else(not_found, |file| Resolution::File { file })
    }

    /// Where a path goes on from `file` when its next segment is no module there and more follows: `grep::cli::X` with `pub extern crate grep_cli as cli`, or `clap::builder::X` with `pub use clap_builder::*`. A segment of CamelCase is a type, and what follows it is a member.
    fn forwarded(&self, file: &str, rest: &[&str]) -> Option<Resolution> {
        let (first, tail) = rest.split_first()?;
        if tail.is_empty()
            || self.forwards >= MAX_FORWARDS
            || !first.starts_with(|ch: char| ch.is_lowercase() || ch == '_')
            || !self
                .seen
                .borrow_mut()
                .insert((file.to_owned(), rest.join("::")))
        {
            return None;
        }
        let next = Self {
            forwards: self.forwards + 1,
            ..*self
        };
        let join = |head: &str, names: &[&str]| {
            let mut spec = vec![head];
            spec.extend_from_slice(names);
            spec.join("::")
        };
        if let Some(target) = self
            .aliases
            .names
            .get(file)
            .and_then(|names| names.get(*first))
        {
            return Some(next.path(file, &join(target, tail)));
        }
        self.aliases.globs.get(file)?.iter().find_map(|glob| {
            let found = next.path(file, &join(glob, rest));
            // A glob of a crate without that module reaches no more than the glob itself does.
            (matches!(found, Resolution::File { .. }) && found != next.path(file, glob))
                .then_some(found)
        })
    }
}
