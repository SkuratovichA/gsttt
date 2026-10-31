//! `gsttt update`: installs the newest release over the running gsttt, then brings the Claude Code plugin in line with it.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::cli::{UpdateArgs, VERSION};
use crate::error::CliError;
use crate::streams::{Io, Outcome};

mod plugin;

/// The installer that `curl … | sh` runs, embedded so that there is one installer: the one the xtask tests prove, and the one the binary starts.
const INSTALLER: &str = include_str!("../../../../install.sh");

/// Where the installer puts the binary, which the installer reads and `update` sets.
const INSTALL_DIRECTORY_VARIABLE: &str = "GSTTT_INSTALL_DIR";

/// The version the installer installs, which `update` sets to the one it was given and removes otherwise, because an inherited one would pin the version silently.
const VERSION_VARIABLE: &str = "GSTTT_VERSION";

/// What `update` says after the plugin lines when it changed something there is a plugin to load: a session that is running keeps the binary and the plugin it started with.
const RESTART: &str = "Restart running Claude Code sessions to load the plugin.\n";

/// `gsttt update [VERSION]`: runs the installer over the running binary, then, unless a version was named, updates the plugin in every Claude Code config that has it.
///
/// A version that is named leaves the plugin alone, because Claude Code installs a plugin only at the newest commit of its marketplace, so an older binary cannot get a plugin that matches it. It needs no repository, so it takes no session. It has no answer, so all it says, and all the installer says, goes to stderr and stdout stays empty.
pub(super) fn update(args: &UpdateArgs, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    let directory = install_directory(
        std::env::var_os(INSTALL_DIRECTORY_VARIABLE),
        std::env::current_exe().and_then(fs::canonicalize),
        VERSION,
    )?;
    let installing = args.version.as_deref().map_or_else(
        || "the newest release".to_owned(),
        |version| format!("gsttt {version}"),
    );
    io.complain(
        format!(
            "Installing {installing} into {} (this is gsttt {VERSION})\n",
            directory.display()
        )
        .as_bytes(),
    );
    let before = installed_version(&directory);
    install(&directory, args.version.as_deref())?;
    let installed = installed_version(&directory) != before;

    if args.version.is_some() {
        io.complain(b"The plugin follows the newest release, so it is left as it is.\n");
        if installed && plugin::is_installed() {
            io.complain(RESTART.as_bytes());
        }
        return Ok(Outcome::Answered);
    }
    plugin::update(io, installed)
}

/// What the `gsttt` in `directory` prints for `--version`, `None` when there is none or it does not run. The installer decides by the same line whether it has anything to install.
fn installed_version(directory: &Path) -> Option<String> {
    let output = Command::new(directory.join("gsttt"))
        .arg("--version")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The directory the installer puts the binary in: the one `variable`, the value of `GSTTT_INSTALL_DIR`, names, or else the one the running binary is in, `exe` being its canonical path or why that is not known.
///
/// A development build, one whose `version` ends with `-dev`, has no directory of its own to be replaced in: a binary built from source belongs to cargo, and a release put over it behind cargo's back is a surprise.
fn install_directory(
    variable: Option<OsString>,
    exe: io::Result<PathBuf>,
    version: &str,
) -> Result<PathBuf, CliError> {
    if let Some(directory) = variable.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(directory));
    }
    if version.ends_with("-dev") {
        return Err(CliError::UpdateOfDevelopmentBuild);
    }
    let exe = exe.map_err(|source| CliError::UpdateDirectory { source })?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| CliError::UpdateDirectory {
            source: io::Error::other(format!("`{}` is in no directory", exe.display())),
        })
}

/// Runs the installer to put `version`, or the newest release, in `directory`. Its stderr is ours and so is its stdin, and its stdout goes to our stderr, because `update` has no answer to put on stdout.
fn install(directory: &Path, version: Option<&str>) -> Result<(), CliError> {
    let mut installer = Command::new("sh");
    installer
        .arg("-c")
        .arg(INSTALLER)
        .arg("install.sh")
        .env(INSTALL_DIRECTORY_VARIABLE, directory)
        .stdout(Stdio::from(io::stderr()));
    match version {
        Some(version) => installer.env(VERSION_VARIABLE, version),
        None => installer.env_remove(VERSION_VARIABLE),
    };
    let status = installer
        .status()
        .map_err(|source| CliError::UpdateInstallerStart { source })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::UpdateInstaller { status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> PathBuf {
        PathBuf::from("/opt/gsttt/bin/gsttt")
    }

    #[test]
    fn the_variable_names_the_directory_even_for_a_development_build() {
        let found = install_directory(Some("/srv/tools".into()), Ok(exe()), "0.1.0-dev").unwrap();
        assert_eq!(found, Path::new("/srv/tools"));
    }

    #[test]
    fn the_directory_of_the_binary_is_used_when_the_variable_is_unset_or_empty() {
        for variable in [None, Some(OsString::new())] {
            let found = install_directory(variable, Ok(exe()), "0.1.42").unwrap();
            assert_eq!(found, Path::new("/opt/gsttt/bin"));
        }
    }

    #[test]
    fn a_development_build_is_not_replaced_without_a_directory_to_replace_it_in() {
        for variable in [None, Some(OsString::new())] {
            let error = install_directory(variable, Ok(exe()), "0.1.0-dev").unwrap_err();
            assert!(
                matches!(error, CliError::UpdateOfDevelopmentBuild),
                "{error:?}"
            );
        }
    }

    #[test]
    fn a_binary_whose_path_is_not_known_has_no_directory() {
        let unknown = Err(io::Error::from(io::ErrorKind::NotFound));
        let error = install_directory(None, unknown, "0.1.42").unwrap_err();
        assert!(
            matches!(error, CliError::UpdateDirectory { .. }),
            "{error:?}"
        );
    }
}
