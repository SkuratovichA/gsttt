//! The workspace as `cargo metadata` describes it: where the targets whose tests run have their
//! crate roots. The proofs of the gate are looked for in the sources those roots reach, nowhere else.

use std::path::{Component, Path, PathBuf};

use anyhow::Context as _;
use serde::Deserialize;
use xshell::{Shell, cmd};

/// A target whose tests `cargo test` runs: a library, a binary or a test target, with its tests on.
/// Examples and benchmarks are left out unless their manifest turns their tests on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    /// The name cargo gives the target.
    pub(super) name: String,
    /// The source file of the crate root, without `.` or `..` in it.
    pub(super) root: PathBuf,
    /// The features without which cargo does not build the target.
    pub(super) features: Vec<String>,
}

/// A member of the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Package {
    /// The directory of its manifest.
    pub(super) dir: PathBuf,
    /// Sorted by crate root.
    pub(super) targets: Vec<Target>,
}

/// The members of the workspace and the directory that holds `openspec/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Workspace {
    pub(super) root: PathBuf,
    /// Sorted by directory.
    pub(super) packages: Vec<Package>,
}

impl Workspace {
    /// Asks cargo about the workspace that holds `dir`.
    pub(super) fn load(dir: &Path) -> anyhow::Result<Self> {
        let sh = Shell::new().context("starting cargo metadata")?;
        sh.change_dir(dir);
        let json = cmd!(sh, "cargo metadata --no-deps --format-version 1")
            .read()
            .with_context(|| format!("running cargo metadata in {}", dir.display()))?;
        Self::parse(&json)
    }

    /// Reads the output of `cargo metadata --no-deps --format-version 1`.
    pub(super) fn parse(json: &str) -> anyhow::Result<Self> {
        let metadata: Metadata =
            serde_json::from_str(json).context("reading the output of cargo metadata")?;
        let mut packages = Vec::new();
        for package in metadata.packages {
            let dir = package.manifest_path.parent().with_context(|| {
                format!(
                    "the manifest path {} has no directory",
                    package.manifest_path.display()
                )
            })?;
            let mut targets: Vec<Target> = package
                .targets
                .into_iter()
                .filter(|target| target.test)
                .map(|target| Target {
                    name: target.name,
                    root: clean(&target.src_path),
                    features: target.required_features,
                })
                .collect();
            targets.sort_by(|a, b| a.root.cmp(&b.root));
            packages.push(Package {
                dir: dir.to_path_buf(),
                targets,
            });
        }
        packages.sort_by(|a, b| a.dir.cmp(&b.dir));
        Ok(Self {
            root: metadata.workspace_root,
            packages,
        })
    }
}

/// `path` with its `.` and `..` components resolved by name alone, as a path in a diagnostic reads
/// best: a `[[test]]` path or a `#[path]` attribute may climb out of a directory and back.
pub(super) fn clean(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other),
        }
    }
    clean
}

#[derive(Deserialize)]
struct Metadata {
    workspace_root: PathBuf,
    packages: Vec<Described>,
}

#[derive(Deserialize)]
struct Described {
    manifest_path: PathBuf,
    targets: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    name: String,
    src_path: PathBuf,
    /// Whether `cargo test` builds the target to run its tests.
    test: bool,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_are_resolved_by_name() {
        assert_eq!(
            clean(Path::new("/repo/crates/app/../../shared/./outside.rs")),
            Path::new("/repo/shared/outside.rs")
        );
        assert_eq!(clean(Path::new("/..")), Path::new("/"));
    }

    #[test]
    fn metadata_without_a_root_is_refused() {
        let error = Workspace::parse(r#"{"packages": []}"#).unwrap_err();
        assert!(format!("{error:#}").contains("workspace_root"), "{error:#}");
    }

    #[test]
    fn only_targets_with_tests_on_are_kept() {
        let json = r#"{
            "workspace_root": "/repo",
            "packages": [{
                "manifest_path": "/repo/crates/app/Cargo.toml",
                "targets": [
                    {"name": "app", "src_path": "/repo/crates/app/src/lib.rs", "test": true},
                    {"name": "demo", "src_path": "/repo/crates/app/examples/demo.rs", "test": false},
                    {"name": "slow", "src_path": "/repo/crates/app/tests/../tests/slow.rs", "test": true,
                     "required-features": ["slow"]}
                ]
            }]
        }"#;
        let workspace = Workspace::parse(json).unwrap();
        assert_eq!(workspace.root, Path::new("/repo"));
        let [package] = workspace.packages.as_slice() else {
            panic!("one package expected: {workspace:?}");
        };
        assert_eq!(package.dir, Path::new("/repo/crates/app"));
        let found: Vec<(&str, &Path, &[String])> = package
            .targets
            .iter()
            .map(|target| {
                (
                    target.name.as_str(),
                    target.root.as_path(),
                    target.features.as_slice(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("app", Path::new("/repo/crates/app/src/lib.rs"), &[][..]),
                (
                    "slow",
                    Path::new("/repo/crates/app/tests/slow.rs"),
                    &["slow".to_owned()][..]
                ),
            ]
        );
    }
}
