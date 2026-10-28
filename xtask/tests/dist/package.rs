//! `cargo xtask package`.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

use crate::support::{printed, repository, xtask_command};

const TARGET: &str = "x86_64-unknown-linux-musl";

/// A file of an archive: its name, permissions and content.
type Entry = (String, u32, Vec<u8>);

/// A target directory holding a release build of `TARGET`, as `cargo build --profile dist` leaves it.
fn built(binary: &[u8]) -> TempDir {
    let target_dir = tempfile::tempdir().unwrap();
    let directory = target_dir.path().join(TARGET).join("dist");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("gsttt"), binary).unwrap();
    target_dir
}

fn package(repo: &Path, target_dir: &Path, target: &str) -> std::process::Output {
    xtask_command(repo)
        .env("CARGO_TARGET_DIR", target_dir)
        .args(["package", "--target", target])
        .output()
        .unwrap()
}

fn entries(archive: &Path) -> Vec<Entry> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive).unwrap()));
    archive
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().into_owned();
            let mode = entry.header().mode().unwrap();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            (name, mode, content)
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02x}").unwrap();
            hex
        })
}

#[gsttt_scenario::scenario("distribution", "A package holds the binary and its checksum")]
#[test]
fn a_package_holds_the_binary_and_its_checksum() {
    let repo = repository("0.1.0");
    let target_dir = built(b"binary bytes");

    let output = package(repo.path(), target_dir.path(), TARGET);

    assert!(output.status.success(), "{}", printed(&output));
    let name = format!("gsttt-0.1.1-{TARGET}.tar.gz");
    let archive = target_dir.path().join("packages").join(&name);
    let entries = entries(&archive);
    let listed: Vec<_> = entries
        .iter()
        .map(|(name, mode, content)| (name.as_str(), *mode & 0o777, content.as_slice()))
        .collect();
    assert_eq!(
        listed,
        [
            ("gsttt", 0o755, b"binary bytes".as_slice()),
            ("LICENSE", 0o644, b"license text\n".as_slice()),
            ("README.md", 0o644, b"readme text\n".as_slice()),
        ]
    );
    let checksum = std::fs::read_to_string(
        target_dir
            .path()
            .join("packages")
            .join(format!("{name}.sha256")),
    )
    .unwrap();
    assert_eq!(
        checksum,
        format!("{}  {name}\n", sha256(&std::fs::read(&archive).unwrap()))
    );
}

#[test]
fn one_commit_packs_to_the_same_bytes() {
    let repo = repository("0.1.0");
    let target_dir = built(b"binary bytes");
    let archive = target_dir
        .path()
        .join("packages")
        .join(format!("gsttt-0.1.1-{TARGET}.tar.gz"));

    assert!(
        package(repo.path(), target_dir.path(), TARGET)
            .status
            .success()
    );
    let first = std::fs::read(&archive).unwrap();
    assert!(
        package(repo.path(), target_dir.path(), TARGET)
            .status
            .success()
    );

    assert_eq!(std::fs::read(&archive).unwrap(), first);
}

#[test]
fn a_target_that_is_not_released_is_refused() {
    let repo = repository("0.1.0");
    let target_dir = built(b"binary bytes");

    let output = package(repo.path(), target_dir.path(), "x86_64-pc-windows-msvc");

    assert!(!output.status.success());
    assert!(
        printed(&output).contains("not a release target"),
        "{}",
        printed(&output)
    );
}

#[test]
fn a_missing_release_build_says_how_to_make_it() {
    let repo = repository("0.1.0");
    let target_dir = tempfile::tempdir().unwrap();

    let output = package(repo.path(), target_dir.path(), TARGET);

    assert!(!output.status.success());
    assert!(
        printed(&output).contains("cargo build --profile dist"),
        "{}",
        printed(&output)
    );
}
