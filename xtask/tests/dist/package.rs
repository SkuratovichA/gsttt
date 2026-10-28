//! `cargo xtask package`.

use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use crate::support::{TARGET, built, package, printed, repository};

/// A file of an archive: its name, permissions and content.
type Entry = (String, u32, Vec<u8>);

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

/// The header of every file of an archive.
fn headers(archive: &Path) -> Vec<tar::Header> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive).unwrap()));
    archive
        .entries()
        .unwrap()
        .map(|entry| entry.unwrap().header().clone())
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    faster_hex::hex_string(&Sha256::digest(bytes))
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
    // The test and the code under test share one hex encoder, so the shape of the line is read on its own: 64 lower-case hex digits, two spaces and the name of the archive.
    let (digest, listed) = checksum
        .strip_suffix('\n')
        .unwrap()
        .split_once("  ")
        .unwrap();
    assert_eq!(digest.len(), 64, "{checksum:?}");
    assert!(
        digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "{checksum:?}"
    );
    assert_eq!(listed, name);
}

/// Packing twice gives the same bytes. This alone would also hold for a header that holds the time of the packing, as long as both runs fall in the same second, so `the_archive_holds_nothing_of_the_machine_or_the_moment` pins the fields.
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
fn the_archive_holds_nothing_of_the_machine_or_the_moment() {
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

    // The gzip header: the time of the compression (bytes 4 to 7) is 0, and the operating system (byte 9) is "unknown" (255), not the one that packed.
    let bytes = std::fs::read(&archive).unwrap();
    assert_eq!(bytes.get(4..8), Some([0, 0, 0, 0].as_slice()));
    assert_eq!(bytes.get(9), Some(&255));
    // The tar headers: the ustar format, with no modification time, owner or group.
    let headers = headers(&archive);
    assert_eq!(headers.len(), 3);
    for header in &headers {
        let name = header.path().unwrap();
        let ustar = header.as_ustar();
        assert!(ustar.is_some(), "{name:?} is not in the ustar format");
        assert_eq!(header.mtime().unwrap(), 0, "the time of {name:?}");
        assert_eq!(header.uid().unwrap(), 0, "the owner id of {name:?}");
        assert_eq!(header.gid().unwrap(), 0, "the group id of {name:?}");
        assert_eq!(
            ustar.map(tar::UstarHeader::username_bytes),
            Some(b"".as_slice()),
            "the owner name of {name:?}"
        );
        assert_eq!(
            ustar.map(tar::UstarHeader::groupname_bytes),
            Some(b"".as_slice()),
            "the group name of {name:?}"
        );
    }
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
