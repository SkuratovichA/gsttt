//! Why a config does not become a setup.

/// Why `.gsttt/init.lua` does not evaluate into a setup.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// What Lua or the gsttt module said, as it said it: a syntax error, a runtime error, a refused call, or the limit that stopped the run. The message starts with the file and line, `.gsttt/init.lua:3: …`, or with the module's file when the error began there.
    #[error("{0}")]
    Lua(String),
    /// The file exists and cannot be read.
    #[error("cannot read {path}: {message}")]
    Read { path: String, message: String },
    /// The Lua state could not be set up.
    #[error("cannot start the Lua sandbox: {0}")]
    Sandbox(String),
}
