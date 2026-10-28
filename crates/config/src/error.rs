//! Why the files of the config do not become a setup.

/// Why `.gsttt/config.toml` and the packs next to it do not read into a setup.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// A file reads and is wrong: a syntax error, a key or a value the setup does not take, or a pack that does not match its file name. The message is `<file>:<line>:<column>: <message>`, cut at 1 KiB.
    #[error("{0}")]
    Invalid(String),
    /// A file exists and cannot be read: it resolves outside the repository, is not a regular file, is too large, is not UTF-8 text or cannot be opened.
    #[error("cannot read {path}: {message}")]
    Read { path: String, message: String },
}
