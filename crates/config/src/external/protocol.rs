//! The messages of the external connector protocol.
//!
//! A message is one JSON object on one line. gsttt writes a [`Request`] to the standard input of the process and reads exactly one [`Response`] from its standard output before it writes the next, so a process answers in the order it is asked. What the process writes to standard error goes to the log.
//!
//! ```text
//! → {"type":"open","protocol":1,"root":"","options":{}}
//! ← {"type":"ready","name":"kotlin","version":"0.1.0"}
//! → {"type":"parse","path":"src/A.kt","text":"…"}
//! ← {"type":"facts","path":"src/A.kt","facts":{…}}
//! → {"type":"resolve","from":"src/A.kt","import":{…}}
//! ← {"type":"resolution","from":"src/A.kt","import":{…},"resolution":{"to":"file","path":"src/B.kt"}}
//! → {"type":"done"}
//! ```
//!
//! Facts and imports are the serde forms of the core types, and files are named by repository-relative path with `/` separators, never by id. What the connector provides is not in the protocol: the config declares it, so that a run knows it without starting the process.

use gsttt_core::fact::{FileFacts, Import, UnresolvedReason};
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The version of the protocol, sent in `open`. It changes when a message changes in a way that an old process would misread.
pub const PROTOCOL: u32 = 1;

/// A message from gsttt to the process.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Request<'a> {
    /// The first message of a run.
    Open {
        /// The version of the protocol, `1`.
        protocol: u32,
        /// The directory the project owns, relative to the repository root: `""` for the root itself. The process runs in the repository root.
        root: &'a str,
        /// The `options` of the connector's declaration in the config, always an object: `{}` when it has none.
        options: &'a Map<String, Value>,
    },
    /// Read one file.
    Parse {
        /// The path of the file, relative to the repository root.
        path: &'a str,
        /// The text of the file, which is UTF-8 and within the size limit.
        text: &'a str,
    },
    /// Where one import of a parsed file points.
    Resolve {
        /// The path of the file that holds the import.
        from: &'a str,
        /// An `Import` of the facts of `from`, in the serde form of the core type: `spec`, `kind`, `line`, `flags` and `names`. The answer repeats it.
        // The core fact types do not derive `JsonSchema` yet; once they do, this and the other substitutes below name the type.
        #[schemars(with = "serde_json::Value")]
        import: &'a Import,
    },
    /// The end of the run: the process should exit.
    Done,
}

/// A message from the process to gsttt, the answer to the request before it.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Response {
    /// The answer to `open`.
    Ready {
        /// The name of the connector.
        name: String,
        /// The version of the connector.
        version: String,
    },
    /// The answer to `parse`.
    Facts {
        /// The path of the file, as it was asked.
        path: String,
        /// What the process read out of the file: a `FileFacts` in the serde form of the core type, with `status`, `tags`, `imports`, `contracts`, `embedded` and `unresolved`.
        #[schemars(with = "serde_json::Value")]
        facts: FileFacts,
    },
    /// The answer to `resolve`. It repeats the file and the import it answers, so that a line that arrives late or twice is never taken for the answer to another import.
    Resolution {
        /// The path of the file that holds the import, as it was asked.
        from: String,
        /// The import, as it was asked: the `import` of the request, copied back unchanged.
        #[schemars(with = "serde_json::Value")]
        import: Import,
        /// Where the import points.
        resolution: WireResolution,
    },
}

impl Response {
    /// What the message is, for a failure that says what came where something else was due.
    pub(super) fn describe(&self) -> String {
        match self {
            Self::Ready { .. } => "ready".to_owned(),
            Self::Facts { path, .. } => format!("facts for `{path}`"),
            Self::Resolution { from, import, .. } => resolution_of(from, import),
        }
    }
}

/// How a failure names the resolution of `import` in `from`. The line tells two imports of the same specifier apart.
pub(super) fn resolution_of(from: &str, import: &Import) -> String {
    format!(
        "resolution of `{}` at line {} in `{from}`",
        import.spec, import.line
    )
}

/// Where an import points, with files named by path where the core type names them by id.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(tag = "to", rename_all = "kebab-case")]
pub enum WireResolution {
    /// A file of the repository.
    File {
        /// The path of the file.
        path: String,
    },
    /// Several files, such as a Python package that reaches its `__init__.py` and the modules it imports by name.
    Files {
        /// The paths of the files.
        paths: Vec<String>,
    },
    /// A third-party package, by its registry name.
    External {
        /// The registry name of the package.
        package: String,
    },
    /// The standard library or the runtime of the language.
    Builtin,
    /// The process knows it cannot resolve the import, and says why: `{"to":"unresolved","reason":"not-found"}`, or with a `detail` for the reasons that carry one.
    Unresolved(Reason),
}

/// Why an import is unresolved. These are the reasons of core's `UnresolvedReason`, written the way it serializes them; core's type has no schema, so a test pins the two against each other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "reason", content = "detail", rename_all = "kebab-case")]
pub enum Reason {
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
    /// A construct the process does not handle yet, named so coverage can count it.
    Unsupported(String),
    /// A project file (`tsconfig.json`, `pyproject.toml`, `Cargo.toml`) the process could not read.
    Config(String),
}

impl From<Reason> for UnresolvedReason {
    fn from(reason: Reason) -> Self {
        match reason {
            Reason::NotFound => Self::NotFound,
            Reason::NonLiteral => Self::NonLiteral,
            Reason::Ignored => Self::Ignored,
            Reason::Generated => Self::Generated,
            Reason::UnknownPackage => Self::UnknownPackage,
            Reason::Unsupported(construct) => Self::Unsupported(construct),
            Reason::Config(message) => Self::Config(message),
        }
    }
}

/// One line of the protocol: a request from gsttt or the response of the process.
#[derive(JsonSchema)]
#[serde(untagged)]
#[expect(dead_code, reason = "never built: `schema` reads its schema")]
enum Message<'a> {
    Request(Request<'a>),
    Response(Response),
}

/// The JSON schema of the messages, which `gsttt schema external` prints.
#[must_use]
pub fn schema() -> Schema {
    schema_for!(Message<'_>)
}

#[cfg(test)]
mod tests {
    use gsttt_core::fact::ImportKind;
    use serde_json::json;

    use super::*;

    fn line(request: &Request<'_>) -> String {
        serde_json::to_string(request).unwrap()
    }

    #[test]
    fn requests_are_the_lines_the_architecture_shows() {
        let options = Map::new();
        assert_eq!(
            line(&Request::Open {
                protocol: PROTOCOL,
                root: "",
                options: &options
            }),
            r#"{"type":"open","protocol":1,"root":"","options":{}}"#
        );
        assert_eq!(
            line(&Request::Parse {
                path: "src/A.kt",
                text: "a\nb"
            }),
            r#"{"type":"parse","path":"src/A.kt","text":"a\nb"}"#
        );
        let import = Import::new("./B", ImportKind::Static, 3);
        assert_eq!(
            line(&Request::Resolve {
                from: "src/A.kt",
                import: &import
            }),
            r#"{"type":"resolve","from":"src/A.kt","import":{"spec":"./B","kind":"static","line":3,"flags":0,"names":[]}}"#
        );
        assert_eq!(line(&Request::Done), r#"{"type":"done"}"#);
    }

    #[test]
    fn responses_read_back() {
        let ready: Response = serde_json::from_value(
            json!({ "type": "ready", "name": "kotlin", "version": "0.1.0" }),
        )
        .unwrap();
        assert!(matches!(ready, Response::Ready { name, .. } if name == "kotlin"));
        let import = Import::new("./B", ImportKind::Static, 3);
        let resolution: Response = serde_json::from_value(json!({
            "type": "resolution",
            "from": "src/A.kt",
            "import": serde_json::to_value(&import).unwrap(),
            "resolution": { "to": "builtin" },
        }))
        .unwrap();
        assert!(matches!(
            resolution,
            Response::Resolution { from, import: echoed, resolution }
                if from == "src/A.kt" && echoed == import && resolution == WireResolution::Builtin
        ));
        assert!(
            serde_json::from_value::<Response>(
                json!({ "type": "resolution", "resolution": { "to": "builtin" } })
            )
            .is_err(),
            "a resolution that does not say what it answers is not a message"
        );
        let facts: Response = serde_json::from_value(json!({
            "type": "facts",
            "path": "src/A.kt",
            "facts": serde_json::to_value(FileFacts::default()).unwrap(),
        }))
        .unwrap();
        assert!(
            matches!(facts, Response::Facts { path, facts } if path == "src/A.kt" && facts == FileFacts::default())
        );
        assert!(
            serde_json::from_value::<Response>(json!({ "type": "parse", "path": "x" })).is_err(),
            "a request is not a response"
        );
    }

    #[test]
    fn resolutions_name_files_by_path_and_keep_the_core_reasons() {
        let read = |value: Value| serde_json::from_value::<WireResolution>(value).unwrap();
        assert_eq!(
            read(json!({ "to": "file", "path": "src/B.kt" })),
            WireResolution::File {
                path: "src/B.kt".to_owned()
            }
        );
        assert_eq!(
            read(json!({ "to": "files", "paths": ["a", "b"] })),
            WireResolution::Files {
                paths: vec!["a".to_owned(), "b".to_owned()]
            }
        );
        assert_eq!(
            read(json!({ "to": "external", "package": "okio" })),
            WireResolution::External {
                package: "okio".to_owned()
            }
        );
        assert_eq!(read(json!({ "to": "builtin" })), WireResolution::Builtin);
        assert_eq!(
            read(json!({ "to": "unresolved", "reason": "not-found" })),
            WireResolution::Unresolved(Reason::NotFound)
        );
        assert_eq!(
            read(json!({ "to": "unresolved", "reason": "unsupported", "detail": "macro" })),
            WireResolution::Unresolved(Reason::Unsupported("macro".to_owned()))
        );
    }

    #[test]
    fn reasons_are_written_the_way_core_writes_them() {
        // The match is exhaustive over core's enum: a reason added there stops this test from compiling until the protocol has it too.
        let mirror = |reason: &UnresolvedReason| match reason {
            UnresolvedReason::NotFound => Reason::NotFound,
            UnresolvedReason::NonLiteral => Reason::NonLiteral,
            UnresolvedReason::Ignored => Reason::Ignored,
            UnresolvedReason::Generated => Reason::Generated,
            UnresolvedReason::UnknownPackage => Reason::UnknownPackage,
            UnresolvedReason::Unsupported(construct) => Reason::Unsupported(construct.clone()),
            UnresolvedReason::Config(message) => Reason::Config(message.clone()),
        };
        for core in [
            UnresolvedReason::NotFound,
            UnresolvedReason::NonLiteral,
            UnresolvedReason::Ignored,
            UnresolvedReason::Generated,
            UnresolvedReason::UnknownPackage,
            UnresolvedReason::Unsupported("macro".to_owned()),
            UnresolvedReason::Config("tsconfig.json".to_owned()),
        ] {
            let wire = mirror(&core);
            assert_eq!(
                serde_json::to_value(&wire).unwrap(),
                serde_json::to_value(&core).unwrap()
            );
            assert_eq!(UnresolvedReason::from(wire), core);
        }
    }
}
