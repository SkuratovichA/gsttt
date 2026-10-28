//! Facts: what a connector reads out of one file, before anything is resolved.

use std::fmt;
use std::ops::BitOr;

use serde::{Deserialize, Serialize};

use crate::contract::{ContractKey, Role};
use crate::ids::FileId;

/// A 1-based line number.
pub type Line = u32;

/// Declares a set of bit flags over a `u8`.
macro_rules! flags {
    ($(#[$meta:meta])* $name:ident { $($(#[$flag_meta:meta])* $flag:ident = $bit:literal;)* }) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
            rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
        )]
        #[serde(transparent)]
        #[rkyv(derive(Debug, PartialEq, Eq))]
        pub struct $name(u8);

        impl $name {
            pub const NONE: Self = Self(0);
            $($(#[$flag_meta])* pub const $flag: Self = Self(1 << $bit);)*

            /// Whether every flag of `other` is set.
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            #[must_use]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// The flags set in both.
            #[must_use]
            pub const fn intersection(self, other: Self) -> Self {
                Self(self.0 & other.0)
            }
        }

        impl BitOr for $name {
            type Output = Self;

            fn bitor(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut set = f.debug_set();
                $(if self.contains(Self::$flag) { set.entry(&stringify!($flag)); })*
                set.finish()
            }
        }
    };
}

#[expect(
    missing_debug_implementations,
    reason = "rkyv derives a public resolver next to each flag set, and resolvers are never printed"
)]
mod flag_sets {
    use super::{BitOr, Deserialize, Serialize, fmt};

    flags! {
        /// How an import is used.
        ImportFlags {
            /// Erased at compile time: TS `import type`, `export type *`.
            TYPE_ONLY = 0;
            /// Only taken when a guard holds: Python `try: … except ImportError`, Rust `#[cfg(feature = …)]`.
            OPTIONAL = 1;
            /// Only seen by the type checker: Python `if TYPE_CHECKING:`.
            TYPE_CHECKING = 2;
            /// Only compiled for tests: Rust `#[cfg(test)]`.
            TEST_ONLY = 3;
        }
    }

    flags! {
        /// What a file is, as far as its connector can tell.
        FileTags {
            /// A test: `*.test.ts`, `test_*.py`, `tests/*.rs`, a `__tests__` directory.
            TEST = 0;
            /// Written by a tool: `@generated`, `DO NOT EDIT`, a codegen output directory.
            GENERATED = 1;
            /// A project file a connector reads to resolve others: `tsconfig.json`, `pyproject.toml`, `Cargo.toml`.
            MANIFEST = 2;
        }
    }
}

pub use flag_sets::{FileTags, ImportFlags};

/// The syntax an import was written in.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ImportKind {
    /// `import x from "a"`, `import a`, `from a import x`, `use a::x`.
    Static,
    /// `export { x } from "a"`, `pub use a::x`.
    ReExport,
    /// `export * from "a"`, `pub use a::*`, `from a import *`.
    ReExportAll,
    /// `import "./polyfill"`.
    SideEffect,
    /// `import("a")`, `importlib.import_module("a")`.
    Dynamic,
    /// `require("a")`.
    Require,
    /// `/// <reference path="a" />`, `#[path = "a.rs"]`.
    Reference,
    /// Rust `mod a;`: the parent owns the child file.
    Module,
    /// `include!("a.rs")`, `include_str!("a.sql")`.
    Include,
}

impl ImportKind {
    pub const ALL: [Self; 9] = [
        Self::Static,
        Self::ReExport,
        Self::ReExportAll,
        Self::SideEffect,
        Self::Dynamic,
        Self::Require,
        Self::Reference,
        Self::Module,
        Self::Include,
    ];

    pub const fn bit(self) -> u16 {
        1 << self as u8
    }
}

/// One import as written in a file.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Import {
    /// The specifier as written: `./b`, `react`, `..models`, `crate::db::pool`.
    pub spec: String,
    pub kind: ImportKind,
    pub line: Line,
    pub flags: ImportFlags,
    /// The names imported from `spec` when resolution depends on them: Python `from pkg import sub` may import
    /// the module `pkg/sub.py`. Empty otherwise.
    pub names: Vec<String>,
}

impl Import {
    /// A static import with no flags and no names.
    pub fn new(spec: impl Into<String>, kind: ImportKind, line: Line) -> Self {
        Self {
            spec: spec.into(),
            kind,
            line,
            flags: ImportFlags::NONE,
            names: Vec::new(),
        }
    }
}

/// One site of a contract in a file.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ContractFact {
    pub key: ContractKey,
    pub role: Role,
    pub line: Line,
    /// The rule or module that found it (`nestjs`, `openapi`, `prisma`), so an answer can say how it knows.
    pub via: String,
}

/// A document in another language inside a file: a GraphQL query in a `gql` template, SQL in a string.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Embedded {
    /// The language of the document: `graphql`, `sql`.
    pub lang: String,
    /// The line the document starts on.
    pub line: Line,
    pub text: String,
    /// The rule that found it.
    pub via: String,
}

/// Something a connector saw and could not turn into a fact. Coverage lists it, so a gap is known.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct UnresolvedSite {
    pub line: Line,
    pub reason: UnresolvedReason,
    /// The code in question, cut to one line: `fetch(url)`, `import(name)`.
    pub detail: String,
}

/// Everything one connector read out of one file. Pure data: the parse cache stores it by content hash.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileFacts {
    pub status: ParseStatus,
    pub tags: FileTags,
    pub imports: Vec<Import>,
    pub contracts: Vec<ContractFact>,
    pub embedded: Vec<Embedded>,
    pub unresolved: Vec<UnresolvedSite>,
}

impl FileFacts {
    /// Facts of a file that could not be parsed at all.
    pub fn failed(failure: ParseFailure) -> Self {
        Self {
            status: ParseStatus::Failed(failure),
            ..Self::default()
        }
    }
}

/// How parsing went.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ParseStatus {
    #[default]
    Ok,
    /// The parser recovered from syntax errors; the facts cover what it could read.
    Recovered { errors: u32, first_error_line: Line },
    /// Nothing was read.
    Failed(ParseFailure),
}

/// Why a file could not be parsed.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(tag = "failure", rename_all = "kebab-case")]
pub enum ParseFailure {
    /// The connector panicked; the engine caught it.
    Panicked {
        message: String,
    },
    /// Past the size limit, usually a bundle or generated code.
    TooLarge {
        bytes: u64,
        limit: u64,
    },
    NotUtf8,
    Unreadable {
        message: String,
    },
    /// A syntax error the parser could not recover from.
    Syntax {
        line: Line,
        message: String,
    },
}

/// Where an import points.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(tag = "to", rename_all = "kebab-case")]
pub enum Resolution {
    /// A file of the repository.
    File { file: FileId },
    /// Several files: Python `from pkg import a, b` reaches `pkg/__init__.py`, `pkg/a.py` and `pkg/b.py`.
    Files { files: Vec<FileId> },
    /// A third-party package, by its registry name.
    External { package: String },
    /// The language's standard library or runtime: `node:fs`, `os`, `std`.
    Builtin,
    /// The connector knows it cannot resolve this one.
    Unresolved(UnresolvedReason),
}

impl Resolution {
    /// The repository files this resolution reaches.
    pub fn files(&self) -> &[FileId] {
        match self {
            Self::File { file } => std::slice::from_ref(file),
            Self::Files { files } => files,
            Self::External { .. } | Self::Builtin | Self::Unresolved(_) => &[],
        }
    }
}

/// An import together with where it points.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ResolvedImport {
    pub import: Import,
    pub to: Resolution,
}

/// Why something is unresolved. Every reason is counted in coverage.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(tag = "reason", content = "detail", rename_all = "kebab-case")]
pub enum UnresolvedReason {
    /// A relative or project path that names no file.
    NotFound,
    /// The target is computed at run time: `import(name)`, `fetch(url)`.
    NonLiteral,
    /// The target exists on disk but git ignores it, which usually means a build step wrote it.
    Ignored,
    /// The target is produced by a build step: a Rust macro, `OUT_DIR`, a code generator.
    Generated,
    /// A bare package name found in no manifest and no workspace.
    UnknownPackage,
    /// A construct the connector does not handle yet, named so coverage can count it.
    Unsupported(String),
    /// A project file (`tsconfig.json`, `pyproject.toml`, `Cargo.toml`) the connector could not read.
    Config(String),
}

impl fmt::Display for UnresolvedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("no such file"),
            Self::NonLiteral => f.write_str("computed at run time"),
            Self::Ignored => f.write_str("target is git-ignored"),
            Self::Generated => f.write_str("generated by a build step"),
            Self::UnknownPackage => f.write_str("package is in no manifest"),
            Self::Unsupported(construct) => write!(f, "unsupported: {construct}"),
            Self::Config(message) => write!(f, "config: {message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_combine() {
        let flags = ImportFlags::TYPE_ONLY | ImportFlags::OPTIONAL;
        assert!(flags.contains(ImportFlags::TYPE_ONLY) && !flags.contains(ImportFlags::TEST_ONLY));
        assert_eq!(format!("{flags:?}"), r#"{"TYPE_ONLY", "OPTIONAL"}"#);
        assert!(ImportFlags::NONE.is_empty());
    }

    #[test]
    fn import_kind_bits_are_distinct() {
        let bits = ImportKind::ALL
            .iter()
            .fold(0u16, |acc, kind| acc | kind.bit());
        assert_eq!(bits.count_ones() as usize, ImportKind::ALL.len());
    }
}
