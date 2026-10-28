//! One TypeScript project: a package, or the repository root for code that lies outside every package.

use std::collections::BTreeSet;
use std::sync::Arc;

use gsttt_core::connector::{LinkInput, LinkedFact, Parsed, Project, Provide, SourceFile};
use gsttt_core::fact::{FileFacts, FileTags, Import, ImportKind, ParseFailure, Resolution};
use gsttt_core::rules::Pack;
use gsttt_core::{RepoPath, RepoView};
use xxhash_rust::xxh3::Xxh3;

use crate::config::extends_of;
use crate::extra::TsExtra;
use crate::json;
use crate::options::TypeScriptOptions;
use crate::pathing::{has_segment, is_config_name, is_json};
use crate::shared::Shared;
use crate::source::{is_code, parse_code};
use crate::{link, provides, resolve};

/// Changes whenever the facts this reader produces for the same text and packs could change.
const READER_VERSION: u32 = 2;

/// A TypeScript project and the rule packs active in it.
#[derive(Debug)]
pub struct TsProject {
    root: String,
    shared: Arc<Shared>,
    packs: Vec<Pack>,
    salt: u64,
}

impl TsProject {
    /// One project per root of `repo`, each with the packs its manifest and the root manifest ask for.
    pub(crate) fn open_all(
        repo: &RepoView,
        options: &TypeScriptOptions,
        packs: &[Pack],
    ) -> Vec<Self> {
        let shared = Arc::new(Shared::build(repo, options));
        shared
            .roots
            .iter()
            .map(|root| {
                let dependencies: BTreeSet<&str> = shared
                    .manifests_above(root)
                    .flat_map(|manifest| manifest.dependencies.iter().map(String::as_str))
                    .collect();
                let active: Vec<Pack> = packs
                    .iter()
                    .filter(|pack| {
                        pack.language == "ts" && pack.applies_to(dependencies.iter().copied())
                    })
                    .cloned()
                    .collect();
                Self {
                    root: root.clone(),
                    shared: Arc::clone(&shared),
                    salt: salt_of(&active),
                    packs: active,
                }
            })
            .collect()
    }
}

/// A hash of the reader version and the active packs, so a pack change reads every file again.
fn salt_of(packs: &[Pack]) -> u64 {
    let mut hasher = Xxh3::new();
    hasher.update(&READER_VERSION.to_le_bytes());
    for pack in packs {
        hasher.update(format!("{pack:?}").as_bytes());
        hasher.update(&[0]);
    }
    hasher.digest()
}

impl Project for TsProject {
    type Extra = TsExtra;

    fn root(&self) -> &str {
        &self.root
    }

    fn matches(&self, path: &RepoPath) -> bool {
        if has_segment(path.as_str(), "node_modules") {
            return false;
        }
        let name = path.file_name();
        (is_code(name) || is_config_name(name) || name == "package.json")
            && self.shared.owner(path) == Some(self.root.as_str())
    }

    fn salt(&self) -> u64 {
        self.salt
    }

    fn provides(&self) -> Vec<Provide> {
        provides::provides(&self.packs)
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<TsExtra> {
        let tags = tags_of(file.path);
        if is_json(file.path.file_name()) {
            return parse_json(file.path, file.text, tags);
        }
        let mut parsed = parse_code(file, &self.packs);
        parsed.facts.tags = tags;
        parsed
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        resolve::resolve(&self.shared, &self.root, from, import)
    }

    fn link(&self, input: &LinkInput<'_, TsExtra>) -> Vec<LinkedFact> {
        link::link(input)
    }
}

/// A `package.json` or a tsconfig: tagged as a manifest, and a tsconfig's `extends` are its imports. A file that is not
/// JSON fails alone.
fn parse_json(path: &RepoPath, text: &str, tags: FileTags) -> Parsed<TsExtra> {
    let mut facts = match json::parse(text) {
        Ok(json) => {
            let mut facts = FileFacts::default();
            if is_config_name(path.file_name()) {
                facts.imports = extends_of(&json)
                    .into_iter()
                    .map(|(spec, line)| Import::new(spec, ImportKind::Reference, line))
                    .collect();
            }
            facts
        }
        Err(error) => FileFacts::failed(ParseFailure::Syntax {
            line: error.line,
            message: error.message,
        }),
    };
    facts.tags = tags;
    Parsed::new(facts)
}

/// The tags a file carries by its name and place.
fn tags_of(path: &RepoPath) -> FileTags {
    let name = path.file_name();
    let mut tags = FileTags::NONE;
    if name == "package.json" || is_config_name(name) {
        tags = tags | FileTags::MANIFEST;
    }
    let in_test_dir = ["__tests__", "test", "tests"]
        .iter()
        .any(|dir| has_segment(path.parent(), dir));
    if is_code(name) && (name.contains(".test.") || name.contains(".spec.") || in_test_dir) {
        tags = tags | FileTags::TEST;
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(path: &str) -> FileTags {
        tags_of(&RepoPath::normalize(path).expect("a repository path"))
    }

    #[test]
    fn tests_and_manifests_are_tagged_by_name() {
        assert!(tags("src/a.test.ts").contains(FileTags::TEST));
        assert!(tags("src/__tests__/a.ts").contains(FileTags::TEST));
        assert!(!tags("src/a.ts").contains(FileTags::TEST));
        assert!(tags("packages/ui/package.json").contains(FileTags::MANIFEST));
        assert!(tags("tsconfig.base.json").contains(FileTags::MANIFEST));
        assert!(!tags("package.json").contains(FileTags::TEST));
    }

    fn pack(name: &str) -> Pack {
        Pack {
            name: name.to_owned(),
            language: "ts".to_owned(),
            detect: Vec::new(),
            calls: Vec::new(),
            decorators: Vec::new(),
            chains: Vec::new(),
            embeds: Vec::new(),
            assigns: Vec::new(),
        }
    }

    #[test]
    fn the_salt_follows_the_packs() {
        assert_eq!(salt_of(&[pack("a")]), salt_of(&[pack("a")]));
        assert_ne!(salt_of(&[]), salt_of(&[pack("a")]));
        assert_ne!(salt_of(&[pack("a")]), salt_of(&[pack("b")]));
        assert_ne!(
            salt_of(&[pack("a"), pack("b")]),
            salt_of(&[pack("b"), pack("a")])
        );
    }
}
