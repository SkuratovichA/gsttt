//! Connectors: the modules that read one language or one contract format.
//!
//! A connector is the plug-in point of gsttt. The engine finds the files, hashes them, caches what connectors
//! return, and builds the graph; a connector only says, one file at a time, what the file imports and which
//! contracts it declares, serves or calls, and then where each import points. A connector is written against
//! [`Connector`] and [`Project`] with its own option and per-file types; the engine holds it as a
//! [`DynConnector`] through [`Erased`], so a new language is one crate and one line in the binary's registry.

use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::Xxh3;

use crate::contract::{ContractKey, ContractKind, Role};
use crate::fact::{ContractFact, Embedded, FileFacts, Import, Resolution, ResolvedImport};
use crate::ids::FileId;
use crate::path::RepoPath;
use crate::repo::RepoView;
use crate::rules::Pack;
use crate::text::with_path;

/// What a connector is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    /// The name configs and answers use: `ts`, `python`, `rust`, `openapi`.
    pub name: &'static str,
    /// Bumped whenever `parse` returns something different for the same input, which drops the cached facts.
    pub version: u32,
    /// One line for `gsttt modules`.
    pub summary: &'static str,
    /// Extensions of the files the connector reads, so coverage can say how many such files nothing read when the
    /// connector is off: `["ts", "tsx", "js"]`.
    pub extensions: &'static [&'static str],
}

/// A language or contract-format module, written against its own types.
pub trait Connector: Send + Sync + 'static {
    /// The options a config passes in the table named after the module, `[<name>]`. Their JSON schema is what an editor checks that table with.
    type Options: Serialize + DeserializeOwned + JsonSchema + Default + Send + Sync;
    type Project: Project;

    fn meta(&self) -> Meta;

    /// Whether the repository has anything for this connector. It looks at the file list only, so it is cheap.
    fn detect(&self, repo: &RepoView) -> bool;

    /// The connector's projects in the repository: a TypeScript project per `package.json`, a Rust crate per
    /// `Cargo.toml`, one project at the root for a contract format. `packs` holds the rule packs written for this
    /// connector; each project keeps those its manifest switches on.
    fn open(
        &self,
        repo: &RepoView,
        options: &Self::Options,
        packs: &[Pack],
    ) -> Result<Vec<Self::Project>, ConnectorError>;
}

/// One root of a connector: the unit that owns files, parses them and resolves their imports.
pub trait Project: Send + Sync + 'static {
    /// Per-file data the link phase needs, cached with the facts. `()` for a project without a link phase.
    type Extra: Serialize + DeserializeOwned + Default + Send + Sync;

    /// The directory the project owns, `""` for the repository root, written the way a [`RepoPath`] is: `/`
    /// between segments, no leading or trailing `/`. A file goes to the deepest project of the first connector
    /// that matches it, and a project is only asked about the files under its root.
    fn root(&self) -> &str;

    /// Whether the project reads `path`. Asked only for files under [`Project::root`].
    fn matches(&self, path: &RepoPath) -> bool;

    /// Mixed into the cache key of every file the project parses, on top of what the key already holds: the
    /// connector's name and version, the module's options, every pack the connector received, the project's root,
    /// and the file's path and content. The salt covers whatever else `parse` depends on, such as the packs this
    /// project's manifest switched on or a setting it read from the manifest when it opened, so that a change in
    /// it reads every file of the project again. A project whose parse depends on nothing beyond its options
    /// returns `0`. [`salt_of`] makes a salt out of text.
    fn salt(&self) -> u64;

    /// The contract kinds this project finds, how, and what it cannot see, for coverage.
    fn provides(&self) -> Vec<Provide>;

    /// Reads one file. It never fails: a file it cannot read comes back with a [`ParseStatus`] that says why.
    ///
    /// [`ParseStatus`]: crate::fact::ParseStatus
    fn parse(&self, file: &SourceFile<'_>) -> Parsed<Self::Extra>;

    /// Runs once every file is parsed and before any is resolved, with the facts of this project's files: a Rust
    /// crate builds its module tree here.
    fn prepare(&mut self, files: &[PrepareFile<'_>]) {
        let _ = files;
    }

    /// Where an import of the file `from` points.
    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution;

    /// Contract sites that only show across files: route prefixes mounted in another file, tRPC routers merged
    /// into an app router. A fact that keys an own fact of the project again is made by [`LinkedFact::rekeyed`], and
    /// the engine drops the site of that own fact and of no other.
    fn link(&self, input: &LinkInput<'_, Self::Extra>) -> Vec<LinkedFact> {
        let _ = input;
        Vec::new()
    }

    /// Languages of embedded documents this project reads: `["graphql"]`.
    fn embeds(&self) -> &[&str] {
        &[]
    }

    /// Reads an embedded document that a file of any connector carries.
    fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact> {
        let _ = document;
        Vec::new()
    }
}

/// A salt for [`Project::salt`] made of text: a hash of `parts` in which each part is taken with its length, so
/// `["ab", "c"]` and `["a", "bc"]` are two salts. The same parts give the same salt in every run and on every
/// machine.
pub fn salt_of(parts: &[&str]) -> u64 {
    let mut hasher = Xxh3::new();
    for part in parts {
        feed(&mut hasher, part.as_bytes());
    }
    hasher.digest()
}

/// Hashes `bytes` with their length in front, so two fields cannot trade bytes and keep the same hash. The one
/// framing behind [`salt_of`] and the fact keys of the engine.
pub(crate) fn feed(hasher: &mut Xxh3, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

/// A file handed to [`Project::parse`].
#[derive(Clone, Copy, Debug)]
pub struct SourceFile<'a> {
    pub id: FileId,
    pub path: &'a RepoPath,
    pub text: &'a str,
}

/// What [`Project::parse`] returns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed<X> {
    pub facts: FileFacts,
    pub extra: X,
}

impl<X: Default> Parsed<X> {
    pub fn new(facts: FileFacts) -> Self {
        Self {
            facts,
            extra: X::default(),
        }
    }
}

/// A parsed file handed to [`Project::prepare`].
#[derive(Clone, Copy, Debug)]
pub struct PrepareFile<'a> {
    pub id: FileId,
    pub path: &'a RepoPath,
    pub facts: &'a FileFacts,
}

/// The files of one project, resolved, handed to [`Project::link`] in id order.
#[derive(Debug)]
pub struct LinkInput<'a, X: ?Sized> {
    files: Vec<LinkFile<'a, X>>,
}

/// One file in a [`LinkInput`].
#[derive(Debug)]
pub struct LinkFile<'a, X: ?Sized> {
    pub id: FileId,
    pub path: &'a RepoPath,
    pub facts: &'a FileFacts,
    pub imports: &'a [ResolvedImport],
    pub extra: &'a X,
}

impl<X: ?Sized> Clone for LinkFile<'_, X> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<X: ?Sized> Copy for LinkFile<'_, X> {}

impl<'a, X: ?Sized> LinkInput<'a, X> {
    pub fn new(mut files: Vec<LinkFile<'a, X>>) -> Self {
        files.sort_unstable_by_key(|file| file.id);
        Self { files }
    }

    pub fn files(&self) -> &[LinkFile<'a, X>] {
        &self.files
    }

    pub fn get(&self, id: FileId) -> Option<&LinkFile<'a, X>> {
        self.files
            .binary_search_by_key(&id, |file| file.id)
            .ok()
            .and_then(|index| self.files.get(index))
    }
}

/// A contract site found by [`Project::link`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedFact {
    pub file: FileId,
    pub fact: ContractFact,
    /// The key of the own fact of `file`, at the line and role of `fact`, that this fact keys again, such as a route
    /// declared on a router and served behind the router's mount. That own fact loses its site and no other does.
    pub replaces: Option<ContractKey>,
}

impl LinkedFact {
    /// A fact the link finds on its own, which replaces no own fact.
    pub fn found(file: FileId, fact: ContractFact) -> Self {
        Self {
            file,
            fact,
            replaces: None,
        }
    }

    /// `own` keyed again as `key`, at the line, role and `via` of `own`, in place of `own`. A link calls it once for
    /// every key a declaration has, such as once per mount of a router.
    pub fn rekeyed(file: FileId, own: &ContractFact, key: ContractKey) -> Self {
        Self {
            file,
            fact: ContractFact {
                key,
                role: own.role,
                line: own.line,
                via: own.via.clone(),
            },
            replaces: Some(own.key.clone()),
        }
    }
}

/// One kind of contract a project finds, for coverage: `http calls via fetch, axios; blind to URLs built at run time`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Provide {
    /// The family of contract: `http`, `graphql`, `grpc`, `trpc`, `table`, `queue`, `env`, or a name of your own.
    // serde reads a kind from a string, which the derive cannot see; the rules say the same.
    #[schemars(with = "String")]
    pub kind: ContractKind,
    /// What the connector does with it: `declares`, `serves` or `calls`.
    pub role: Role,
    /// The packs or formats that find it.
    pub via: Vec<String>,
    /// What it cannot see, in words an answer can quote.
    pub blind_spots: Vec<String>,
}

/// Why a connector could not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectorError {
    #[error("options of `{connector}`: {message}")]
    Options { connector: String, message: String },
    #[error("{connector} project at `{root}`: {message}")]
    Project {
        connector: String,
        root: String,
        message: String,
    },
    #[error("cached link data of `{connector}` does not read back: {message}")]
    Extra { connector: String, message: String },
}

/// A connector as the engine holds it. Implemented by [`Erased`]; write a [`Connector`] instead.
pub trait DynConnector: Send + Sync {
    fn meta(&self) -> Meta;
    fn detect(&self, repo: &RepoView) -> bool;
    /// The JSON schema of the options, for an editor that checks the config. The types it uses are described in `generator` and referenced from the schema returned, so that the schema of a whole config describes each type once.
    fn options_schema(&self, generator: &mut SchemaGenerator) -> Schema;
    /// The default options as JSON.
    fn default_options(&self) -> serde_json::Value;
    /// Opens the projects with options as JSON; `null` means the defaults.
    fn open(
        &self,
        repo: &RepoView,
        options: &serde_json::Value,
        packs: &[Pack],
    ) -> Result<Vec<Box<dyn DynProject>>, ConnectorError>;
}

/// A project as the engine holds it. Implemented through [`Erased`]; write a [`Project`] instead.
pub trait DynProject: Send + Sync {
    fn connector(&self) -> Meta;
    fn root(&self) -> &str;
    fn matches(&self, path: &RepoPath) -> bool;
    fn salt(&self) -> u64;
    fn provides(&self) -> Vec<Provide>;
    /// The facts, and the extra data as the JSON the cache stores (empty for the default value).
    fn parse(&self, file: &SourceFile<'_>) -> (FileFacts, Vec<u8>);
    fn prepare(&mut self, files: &[PrepareFile<'_>]);
    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution;
    /// Links with the extra data as JSON, the way [`DynProject::parse`] gave it.
    fn link(&self, input: &LinkInput<'_, [u8]>) -> Result<Vec<LinkedFact>, ConnectorError>;
    fn embeds(&self) -> &[&str];
    fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact>;
}

impl fmt::Debug for dyn DynConnector + '_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DynConnector")
            .field(&self.meta().name)
            .finish()
    }
}

impl fmt::Debug for dyn DynProject + '_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DynProject")
            .field(&self.connector().name)
            .field(&self.root())
            .finish()
    }
}

/// A [`Connector`] as a [`DynConnector`]. The registry holds `&Erased(TsConnector)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Erased<C>(pub C);

/// A [`Project`] as a [`DynProject`].
struct ErasedProject<P> {
    meta: Meta,
    project: P,
}

impl<C: Connector> DynConnector for Erased<C> {
    fn meta(&self) -> Meta {
        self.0.meta()
    }

    fn detect(&self, repo: &RepoView) -> bool {
        self.0.detect(repo)
    }

    fn options_schema(&self, generator: &mut SchemaGenerator) -> Schema {
        generator.subschema_for::<C::Options>()
    }

    fn default_options(&self) -> serde_json::Value {
        serde_json::to_value(C::Options::default()).unwrap_or(serde_json::Value::Null)
    }

    fn open(
        &self,
        repo: &RepoView,
        options: &serde_json::Value,
        packs: &[Pack],
    ) -> Result<Vec<Box<dyn DynProject>>, ConnectorError> {
        let meta = self.0.meta();
        let options = if options.is_null() {
            C::Options::default()
        } else {
            serde_path_to_error::deserialize(options).map_err(|error| ConnectorError::Options {
                connector: meta.name.to_owned(),
                message: with_path(error.path(), &error.inner().to_string()),
            })?
        };
        let projects = self.0.open(repo, &options, packs)?;
        Ok(projects
            .into_iter()
            .map(|project| -> Box<dyn DynProject> { Box::new(ErasedProject { meta, project }) })
            .collect())
    }
}

impl<P: Project> ErasedProject<P> {
    fn decode(&self, bytes: &[u8]) -> Result<P::Extra, ConnectorError> {
        if bytes.is_empty() {
            return Ok(P::Extra::default());
        }
        serde_json::from_slice(bytes).map_err(|error| ConnectorError::Extra {
            connector: self.meta.name.to_owned(),
            message: error.to_string(),
        })
    }
}

impl<P: Project> DynProject for ErasedProject<P> {
    fn connector(&self) -> Meta {
        self.meta
    }

    fn root(&self) -> &str {
        self.project.root()
    }

    fn matches(&self, path: &RepoPath) -> bool {
        self.project.matches(path)
    }

    fn salt(&self) -> u64 {
        self.project.salt()
    }

    fn provides(&self) -> Vec<Provide> {
        self.project.provides()
    }

    fn parse(&self, file: &SourceFile<'_>) -> (FileFacts, Vec<u8>) {
        let Parsed { facts, extra } = self.project.parse(file);
        if size_of::<P::Extra>() == 0 {
            return (facts, Vec::new());
        }
        let bytes = serde_json::to_vec(&extra).unwrap_or_else(|error| {
            tracing::warn!(connector = self.meta.name, file = %file.path, %error, "link data does not serialise");
            Vec::new()
        });
        (facts, bytes)
    }

    fn prepare(&mut self, files: &[PrepareFile<'_>]) {
        self.project.prepare(files);
    }

    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        self.project.resolve(from, import)
    }

    fn link(&self, input: &LinkInput<'_, [u8]>) -> Result<Vec<LinkedFact>, ConnectorError> {
        let extras = input
            .files()
            .iter()
            .map(|file| self.decode(file.extra))
            .collect::<Result<Vec<P::Extra>, _>>()?;
        let files = input
            .files()
            .iter()
            .zip(&extras)
            .map(|(file, extra)| LinkFile {
                id: file.id,
                path: file.path,
                facts: file.facts,
                imports: file.imports,
                extra,
            })
            .collect();
        Ok(self.project.link(&LinkInput::new(files)))
    }

    fn embeds(&self) -> &[&str] {
        self.project.embeds()
    }

    fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact> {
        self.project.parse_embedded(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fact::{ImportKind, UnresolvedReason};

    #[test]
    fn a_salt_hashes_its_parts_with_their_lengths() {
        // The framing of the fact keys: the length of each part as a little-endian u64, then its bytes.
        let mut hasher = Xxh3::new();
        for part in ["ab", "c"] {
            hasher.update(&u64::try_from(part.len()).unwrap().to_le_bytes());
            hasher.update(part.as_bytes());
        }
        assert_eq!(salt_of(&["ab", "c"]), hasher.digest());
        // Parts cannot trade bytes, and an empty part is still a part.
        assert_ne!(salt_of(&["ab", "c"]), salt_of(&["a", "bc"]));
        assert_ne!(salt_of(&["a"]), salt_of(&["a", ""]));
        assert_ne!(salt_of(&[]), salt_of(&[""]));
        // Pinned, because a changed hash or byte order would read the files of every project again.
        assert_eq!(salt_of(&["a"]), 14_372_996_823_028_516_049);
    }

    fn served(key: &str, line: u32, via: &str) -> ContractFact {
        ContractFact {
            key: ContractKey::parse(key).unwrap(),
            role: Role::Serves,
            line,
            via: via.to_owned(),
        }
    }

    #[test]
    fn a_found_fact_replaces_no_own_fact() {
        let fact = served("http GET /api/a", 3, "express");
        let linked = LinkedFact::found(FileId::from_raw(7), fact.clone());
        assert_eq!(linked.file, FileId::from_raw(7));
        assert_eq!(linked.fact, fact);
        assert_eq!(linked.replaces, None);
    }

    #[test]
    fn a_rekeyed_fact_takes_the_place_of_its_own_fact_and_names_its_key() {
        let own = served("http GET /a", 3, "axum");
        let key = ContractKey::parse("http GET /api/a").unwrap();
        let linked = LinkedFact::rekeyed(FileId::from_raw(7), &own, key.clone());
        assert_eq!(linked.file, FileId::from_raw(7));
        assert_eq!(linked.fact, served("http GET /api/a", 3, "axum"));
        assert_eq!(linked.replaces, Some(own.key));
    }

    /// A connector over `.txt` files whose lines `use <path>` import other files and whose link phase reports a
    /// contract for every file with a non-empty extra.
    struct Lines;

    #[derive(Default, Serialize, Deserialize, JsonSchema)]
    struct LinesOptions {
        #[serde(default)]
        tag: String,
    }

    struct LinesProject {
        repo: RepoView,
        tag: String,
    }

    impl Connector for Lines {
        type Options = LinesOptions;
        type Project = LinesProject;

        fn meta(&self) -> Meta {
            Meta {
                name: "lines",
                version: 1,
                summary: "test",
                extensions: &["txt"],
            }
        }

        fn detect(&self, repo: &RepoView) -> bool {
            repo.files()
                .any(|(_, path)| path.extension() == Some("txt"))
        }

        fn open(
            &self,
            repo: &RepoView,
            options: &LinesOptions,
            _: &[Pack],
        ) -> Result<Vec<LinesProject>, ConnectorError> {
            Ok(vec![LinesProject {
                repo: repo.clone(),
                tag: options.tag.clone(),
            }])
        }
    }

    impl Project for LinesProject {
        type Extra = String;

        fn root(&self) -> &'static str {
            ""
        }

        fn matches(&self, path: &RepoPath) -> bool {
            path.extension() == Some("txt")
        }

        fn salt(&self) -> u64 {
            0
        }

        fn provides(&self) -> Vec<Provide> {
            Vec::new()
        }

        fn parse(&self, file: &SourceFile<'_>) -> Parsed<String> {
            let imports = file
                .text
                .lines()
                .filter_map(|line| line.strip_prefix("use "))
                .map(|spec| Import::new(spec, ImportKind::Static, 1))
                .collect();
            Parsed {
                facts: FileFacts {
                    imports,
                    ..FileFacts::default()
                },
                extra: self.tag.clone(),
            }
        }

        fn resolve(&self, _: &RepoPath, import: &Import) -> Resolution {
            match self.repo.lookup(&import.spec) {
                Some(file) => Resolution::File { file },
                None => Resolution::Unresolved(UnresolvedReason::NotFound),
            }
        }

        fn link(&self, input: &LinkInput<'_, String>) -> Vec<LinkedFact> {
            input
                .files()
                .iter()
                .filter(|file| !file.extra.is_empty())
                .map(|file| {
                    LinkedFact::found(
                        file.id,
                        ContractFact {
                            key: ContractKey::parse(&format!("queue {}", file.extra)).unwrap(),
                            role: Role::Calls,
                            line: 1,
                            via: "lines".into(),
                        },
                    )
                })
                .collect()
        }
    }

    static REGISTRY: &[&dyn DynConnector] = &[&Erased(Lines)];

    #[test]
    fn erased_connectors_round_trip_options_and_extras() {
        let path = |name: &str| RepoPath::normalize(name).unwrap();
        let repo = RepoView::new("/repo", vec![path("a.txt"), path("b.txt")]);
        let [connector] = REGISTRY else {
            panic!("one connector")
        };
        assert!(connector.detect(&repo));

        // The options are a reference into the generator, which holds what they say.
        let mut generator = SchemaGenerator::default();
        let schema = connector.options_schema(&mut generator);
        assert!(schema.get("$ref").is_some(), "{schema:?}");
        let definitions = serde_json::Value::Object(generator.take_definitions(false));
        assert!(definitions.to_string().contains("tag"), "{definitions}");

        let bad = connector.open(&repo, &serde_json::json!({ "tag": 3 }), &[]);
        assert_eq!(
            bad.err(),
            Some(ConnectorError::Options {
                connector: "lines".to_owned(),
                message: "`tag`: invalid type: integer `3`, expected a string".to_owned(),
            })
        );
        let projects = connector
            .open(&repo, &serde_json::json!({ "tag": "jobs" }), &[])
            .unwrap();
        let [project] = projects.as_slice() else {
            panic!("one project")
        };

        let a = path("a.txt");
        let (facts, extra) = project.parse(&SourceFile {
            id: FileId::from_raw(0),
            path: &a,
            text: "use b.txt\nuse c.txt",
        });
        let resolved: Vec<ResolvedImport> = facts
            .imports
            .iter()
            .map(|import| ResolvedImport {
                import: import.clone(),
                to: project.resolve(&a, import),
            })
            .collect();
        assert_eq!(
            resolved[0].to,
            Resolution::File {
                file: FileId::from_raw(1)
            }
        );
        assert_eq!(
            resolved[1].to,
            Resolution::Unresolved(UnresolvedReason::NotFound)
        );

        let input = LinkInput::new(vec![LinkFile {
            id: FileId::from_raw(0),
            path: &a,
            facts: &facts,
            imports: &resolved,
            extra: extra.as_slice(),
        }]);
        let linked = project.link(&input).unwrap();
        assert_eq!(linked[0].fact.key.to_string(), "queue jobs");
        assert_eq!(format!("{project:?}"), r#"DynProject("lines", "")"#);
    }
}
