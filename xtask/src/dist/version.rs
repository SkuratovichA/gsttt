//! `cargo xtask version`: the version of a release.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context as _, bail};

use super::git::git;
use crate::root::repository_root;

/// Prints the version a release of the checked-out commit gets.
pub(super) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    if !args.is_empty() {
        bail!("usage: cargo xtask version");
    }
    let version = release_version(&repository_root()?)?;
    writeln!(std::io::stdout(), "{version}")?;
    Ok(ExitCode::SUCCESS)
}

/// The version of a release of the commit checked out in `root`: `<major>.<minor>` from `[workspace.package] version`, then the number of commits in the history of the commit.
pub(super) fn release_version(root: &Path) -> anyhow::Result<String> {
    let major_minor = major_minor(&root.join("Cargo.toml"))?;
    // A shallow clone counts too few commits, and a release must never be numbered below an earlier one.
    if git(root, &["rev-parse", "--is-shallow-repository"])? == "true" {
        bail!(
            "{} is a shallow clone, so its commits cannot be counted: fetch the whole history",
            root.display()
        );
    }
    let commits = git(root, &["rev-list", "--count", "HEAD"])?;
    Ok(format!("{major_minor}.{commits}"))
}

/// `<major>.<minor>` of the `[workspace.package] version` in the manifest at `manifest`.
fn major_minor(manifest: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(manifest)
        .with_context(|| format!("cannot read {}", manifest.display()))?;
    let parsed: toml::Table = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", manifest.display()))?;
    let version = parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .with_context(|| {
            format!(
                "{} has no `[workspace.package] version`",
                manifest.display()
            )
        })?;
    let mut parts = version.split('.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(major), Some(minor), Some(_)) if is_number(major) && is_number(minor) => {
            Ok(format!("{major}.{minor}"))
        }
        _ => bail!(
            "{}: `[workspace.package] version` is `{version}`, expected `<major>.<minor>.<patch>`",
            manifest.display()
        ),
    }
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::major_minor;

    fn major_minor_of(manifest: &str) -> anyhow::Result<String> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("Cargo.toml");
        std::fs::write(&path, manifest)?;
        major_minor(&path)
    }

    #[test]
    fn takes_major_and_minor_from_the_workspace_version() {
        let manifest = "[workspace.package]\nversion = \"3.14.15\"\n";
        assert_eq!(major_minor_of(manifest).unwrap(), "3.14");
    }

    #[test]
    fn rejects_a_version_that_is_not_three_numbers() {
        for version in ["1.2", "x.2.3", "1..3", "1.2.3"] {
            let manifest = format!("[workspace.package]\nversion = \"{version}\"\n");
            assert_eq!(
                major_minor_of(&manifest).is_ok(),
                version == "1.2.3",
                "{version}"
            );
        }
    }

    #[test]
    fn names_the_manifest_without_a_workspace_version() {
        let error = major_minor_of("[package]\nname = \"x\"\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no `[workspace.package] version`"),
            "{error}"
        );
    }
}
