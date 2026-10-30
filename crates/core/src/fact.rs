//! Facts: what a connector reads out of one file, before anything is resolved.

use std::fmt;
use std::ops::BitOr;

use serde::{Deserialize, Serialize};

use crate::contract::{ContractKey, Role};
use crate::ids::FileId;
use crate::text::thousands;

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
    /// `vi.mock("a")`, `jest.mock("a")`: a test mocks the module, stops mocking it, or makes or sets a mock for it.
    Mock,
}

impl ImportKind {
    pub const ALL: [Self; 10] = [
        Self::Static,
        Self::ReExport,
        Self::ReExportAll,
        Self::SideEffect,
        Self::Dynamic,
        Self::Require,
        Self::Reference,
        Self::Module,
        Self::Include,
        Self::Mock,
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
    /// The names the import takes from `spec`: `a` and `b` in `import { a, b } from 'spec'`, `default` for a default import, `*` for a namespace. Resolution may depend on them too: Python `from pkg import sub` may import the module `pkg/sub.py`. Empty when the connector records none.
    pub names: Vec<String>,
    /// For a named re-export, the name each entry of `names` is exported as: `w` for `export { z as w } from 'spec'`, `ns` for `export * as ns from 'spec'`. A connector may also give, for any import, the name the file binds an entry as when it is not the name taken, which Rust does for `use a::Foo as Bar`. A connector that records [`Symbols`] gives, for every static import of the file, type-only ones included, the local name of each entry of `names` in the same order: `React` for `import React from 'react'`, whose name is `default`, `ns` for `import * as ns`, `b` for `import { a as b }` and `a` for `import { a }`, which is how a walk by name finds the declarations that use what the import takes. An import type, `import('./a').T`, binds no name and gets none, so a walk takes what it names as used where it is written. Empty for every other import.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exported: Vec<String>,
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
            exported: Vec::new(),
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
    /// The names the file exports itself, sorted, without the names an `export *` brings in. `None` when the connector cannot tell, because the file does not list its exports or was read past errors. Empty for a file that exports nothing itself, such as a barrel made only of `export *`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,
    /// The declarations of the file and what each of them refers to, which `impact` follows when it is asked about names. `None` when the connector records none, which every connector but TypeScript does for now, or when the file was read past errors; `impact` follows such a file whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Symbols>,
}

/// The declarations of a file and what each of them refers to, so that `impact` asked about a name can list the declarations that use it instead of every file that imports its file.
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
pub struct Symbols {
    /// The declarations of the file, by the line they start on and then by name, each name once.
    pub declared: Vec<Declared>,
    /// What the file's code refers to as the file loads, sorted, once each: everything in a top-level statement that is not a declaration, callbacks included, and what a declaration calls while it is evaluated, such as `f` in `const x = f()`. A change to one of them can fail the file as it loads, and so every file that loads it.
    pub on_load: Vec<String>,
    /// Each name the file exports from a local of another name, by the name it is exported as: `b` from `a` for `export { a as b }`, `default` from `f` for `export default f`. A name exported under its own local name is not listed, and neither is a re-export, which its import describes.
    pub exports: Vec<LocalExport>,
}

/// What joins a class and a member of it in the name of a declaration: `Runner.start`, and `Runner.#start` for a private member. A connector writes member names with it and a walk by name splits them on it.
pub const MEMBER_SEPARATOR: char = '.';

/// One declaration of a file.
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
pub struct Declared {
    /// The name it declares: `getHash`; `ModuleRunner.import` for a member of a class; `default` for `export default` of an expression. Overloads and merged declarations share one entry.
    pub name: String,
    /// The line it starts on, from 1.
    pub line: Line,
    /// The line it ends on, from 1.
    pub end: Line,
    /// The names it refers to, sorted, once each, never its own: other declarations of the file, `Class.member` for `this.member` inside the class, and the local names the file's imports bind. A top-level variable also lists the declarations that write it.
    pub uses: Vec<String>,
}

/// A name a file exports from a local of another name.
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
pub struct LocalExport {
    /// The name it is exported as: `b` in `export { a as b }`.
    pub name: String,
    /// The declaration or import binding behind it: `a` in `export { a as b }`.
    pub local: String,
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

/// Counts the syntax errors a recovering parser reports for one file and keeps the smallest line among them, because a parser does not always report its errors in the order of the text. [`ErrorCount::status`] turns the count into the [`ParseStatus`] of the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ErrorCount {
    errors: u32,
    first_line: Line,
}

impl ErrorCount {
    /// Counts one error at `line`. The count stops at `u32::MAX`, and the line kept is the smallest one noted.
    pub fn note(&mut self, line: Line) {
        self.first_line = if self.errors == 0 {
            line
        } else {
            self.first_line.min(line)
        };
        self.errors = self.errors.saturating_add(1);
    }

    /// [`ParseStatus::Ok`] when no error was noted, otherwise [`ParseStatus::Recovered`] with the count and the smallest line noted.
    pub fn status(self) -> ParseStatus {
        if self.errors == 0 {
            ParseStatus::Ok
        } else {
            ParseStatus::Recovered {
                errors: self.errors,
                first_error_line: self.first_line,
            }
        }
    }
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

/// How a file failed, in a sentence part: `syntax error at line 3: unexpected token`.
impl ParseFailure {
    /// Whether the engine put the file aside before its connector saw it: past the size limit, not UTF-8 or not readable. Such a file is owned and counted, and it is not a failure of the connector's parse.
    pub fn is_skipped(&self) -> bool {
        match self {
            Self::TooLarge { .. } | Self::NotUtf8 | Self::Unreadable { .. } => true,
            Self::Panicked { .. } | Self::Syntax { .. } => false,
        }
    }
}

impl fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panicked { message } => write!(f, "the parser panicked: {message}"),
            Self::TooLarge { bytes, limit } => write!(
                f,
                "{} bytes, over the limit of {}",
                thousands(*bytes),
                thousands(*limit)
            ),
            Self::NotUtf8 => f.write_str("not valid UTF-8"),
            Self::Unreadable { message } => write!(f, "could not be read: {message}"),
            Self::Syntax { line, message } => write!(f, "syntax error at line {line}: {message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_counted_and_the_smallest_line_is_kept() {
        assert_eq!(ErrorCount::default().status(), ParseStatus::Ok);
        let mut count = ErrorCount::default();
        for line in [9, 4, 7] {
            count.note(line);
        }
        assert_eq!(
            count.status(),
            ParseStatus::Recovered {
                errors: 3,
                first_error_line: 4
            }
        );
    }

    #[test]
    fn the_count_saturates_at_the_largest_u32() {
        let mut count = ErrorCount {
            errors: u32::MAX - 1,
            first_line: 5,
        };
        count.note(8);
        count.note(2);
        assert_eq!(
            count.status(),
            ParseStatus::Recovered {
                errors: u32::MAX,
                first_error_line: 2
            }
        );
    }

    #[test]
    fn flags_combine() {
        let flags = ImportFlags::TYPE_ONLY | ImportFlags::OPTIONAL;
        assert!(flags.contains(ImportFlags::TYPE_ONLY) && !flags.contains(ImportFlags::TEST_ONLY));
        assert_eq!(format!("{flags:?}"), r#"{"TYPE_ONLY", "OPTIONAL"}"#);
        assert!(ImportFlags::NONE.is_empty());
    }

    /// One failure of every kind.
    fn every_failure() -> [ParseFailure; 5] {
        [
            ParseFailure::Panicked {
                message: "boom".to_owned(),
            },
            ParseFailure::TooLarge {
                bytes: 2_000_000,
                limit: 1_000_000,
            },
            ParseFailure::NotUtf8,
            ParseFailure::Unreadable {
                message: "permission denied".to_owned(),
            },
            ParseFailure::Syntax {
                line: 3,
                message: "unexpected token".to_owned(),
            },
        ]
    }

    #[test]
    fn a_failure_is_spelled_as_a_sentence_part() {
        let failures = every_failure();
        let spelled: Vec<String> = failures.iter().map(ToString::to_string).collect();
        assert_eq!(
            spelled,
            [
                "the parser panicked: boom",
                "2,000,000 bytes, over the limit of 1,000,000",
                "not valid UTF-8",
                "could not be read: permission denied",
                "syntax error at line 3: unexpected token",
            ]
        );
    }

    #[test]
    fn a_file_put_aside_is_skipped_and_a_failed_parse_is_not() {
        for failure in every_failure() {
            // A match without a wildcard, so that a new variant does not compile until it is classified here as well.
            let put_aside = match failure {
                ParseFailure::TooLarge { .. }
                | ParseFailure::NotUtf8
                | ParseFailure::Unreadable { .. } => true,
                ParseFailure::Panicked { .. } | ParseFailure::Syntax { .. } => false,
            };
            assert_eq!(failure.is_skipped(), put_aside, "{failure}");
        }
    }

    #[test]
    fn import_kind_bits_are_distinct() {
        let bits = ImportKind::ALL
            .iter()
            .fold(0u16, |acc, kind| acc | kind.bit());
        assert_eq!(bits.count_ones() as usize, ImportKind::ALL.len());
    }
}
