//! `cargo xtask package --target <target>`: packs the release build of a target.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, bail};
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest as _, Sha256};

use super::version::release_version;
use crate::root::repository_root;

const USAGE: &str = "usage: cargo xtask package --target <target>";

/// The targets a release is built for.
const TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

/// A file of a package: the name it has inside the archive, where it comes from and its permissions there.
struct Entry {
    name: &'static str,
    source: PathBuf,
    mode: u32,
}

/// A package on disk.
#[derive(Debug)]
struct Package {
    archive: PathBuf,
    checksum: PathBuf,
}

/// Packs the release build of the target named by `--target` and prints where the archive and its checksum are.
pub(super) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let [flag, target] = args else { bail!(USAGE) };
    if flag != "--target" {
        bail!(USAGE);
    }
    let root = repository_root()?;
    let package = pack(&root, &target_dir(&root)?, target)?;
    let mut stdout = std::io::stdout();
    writeln!(stdout, "{}", package.archive.display())?;
    writeln!(stdout, "{}", package.checksum.display())?;
    Ok(ExitCode::SUCCESS)
}

/// Where cargo puts what it builds for the repository in `root`: `CARGO_TARGET_DIR`, which cargo reads relative to the directory it was started in, else `target` in the repository.
fn target_dir(root: &Path) -> anyhow::Result<PathBuf> {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => Ok(std::env::current_dir()?.join(dir)),
        None => Ok(root.join("target")),
    }
}

/// Packs `<target_dir>/<target>/dist/gsttt` with the `LICENSE` and `README.md` of `root` into `<target_dir>/packages/`.
fn pack(root: &Path, target_dir: &Path, target: &str) -> anyhow::Result<Package> {
    if !TARGETS.contains(&target) {
        bail!(
            "`{target}` is not a release target, expected one of {}",
            TARGETS.join(", ")
        );
    }
    let binary = target_dir.join(target).join("dist").join("gsttt");
    if !binary.is_file() {
        bail!(
            "{} does not exist: build it first with `cargo build --profile dist --target {target} --package gsttt`",
            binary.display()
        );
    }
    let name = format!("gsttt-{}-{target}.tar.gz", release_version(root)?);
    let directory = target_dir.join("packages");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;

    let archive = directory.join(&name);
    let entries = [
        Entry {
            name: "gsttt",
            source: binary,
            mode: 0o755,
        },
        Entry {
            name: "LICENSE",
            source: root.join("LICENSE"),
            mode: 0o644,
        },
        Entry {
            name: "README.md",
            source: root.join("README.md"),
            mode: 0o644,
        },
    ];
    write_archive(&archive, &entries)?;

    // The `sha256sum` format, so `sha256sum --check` and `shasum --algorithm 256 --check` read it as well.
    let bytes = fs::read(&archive).with_context(|| format!("cannot read {}", archive.display()))?;
    let checksum = directory.join(format!("{name}.sha256"));
    let line = format!("{}  {name}\n", hex(&Sha256::digest(&bytes)));
    fs::write(&checksum, line).with_context(|| format!("cannot write {}", checksum.display()))?;
    Ok(Package { archive, checksum })
}

/// The lowercase hexadecimal digits of `bytes`.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// Writes `entries` to `archive` as a gzipped tarball. Times, owners and the gzip header carry nothing of the machine, so one commit packs to the same bytes everywhere.
///
/// The compressed bytes also depend on the deflate backend of `flate2`, today the pure-Rust `miniz_oxide`, and on the compression level. A crate that turns on another backend (the `zlib-ng` or `zlib-rs` feature of `flate2`) anywhere in the same build changes them, because features add up across a build. The checksum is made from the archive that was written, so a release stays consistent either way.
fn write_archive(archive: &Path, entries: &[Entry]) -> anyhow::Result<()> {
    let file =
        File::create(archive).with_context(|| format!("cannot create {}", archive.display()))?;
    let mut builder = tar::Builder::new(GzEncoder::new(file, Compression::best()));
    for entry in entries {
        let data = fs::read(&entry.source)
            .with_context(|| format!("cannot read {}", entry.source.display()))?;
        let mut header = tar::Header::new_ustar();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(u64::try_from(data.len())?);
        header.set_mode(entry.mode);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        builder.append_data(&mut header, entry.name, data.as_slice())?;
    }
    builder.into_inner()?.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TARGETS, hex};

    #[test]
    fn writes_bytes_as_lowercase_hexadecimal() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }

    #[test]
    fn the_targets_are_the_four_of_the_spec() {
        assert_eq!(
            TARGETS,
            [
                "x86_64-unknown-linux-musl",
                "aarch64-unknown-linux-musl",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin"
            ]
        );
    }
}
