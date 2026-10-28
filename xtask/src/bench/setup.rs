//! Setup: the checkouts of the repositories and the installs of the tools. What is already there at the pinned commit or version is reused.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::BufRead as _;
use std::path::Path;

use anyhow::{Context as _, ensure};
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use xshell::Shell;

use super::pins::{Language, Pins, Repository, Tool};
use super::plan::Programs;
use crate::dist::git::git;

/// Installs the tools `needed` that are not installed at their pinned version yet.
pub(super) fn install(
    programs: &Programs,
    pins: &Pins,
    needed: &BTreeSet<Tool>,
) -> anyhow::Result<()> {
    if needed.contains(&Tool::DependencyCruiser) || needed.contains(&Tool::Madge) {
        node_tools(programs, pins)?;
    }
    if needed.contains(&Tool::Grimp) {
        python_tools(programs, pins)?;
    }
    if needed.contains(&Tool::CodebaseMemory) {
        cbm(programs, pins)?;
    }
    Ok(())
}

/// What `<tool> --version` prints, or for grimp, which has no such flag, what its package metadata says: the last word of the first line.
pub(super) fn installed_version(tool: Tool, programs: &Programs) -> anyhow::Result<String> {
    let sh = Shell::new()?;
    let program = programs.of(tool);
    let cmd = match tool {
        Tool::Grimp => sh.cmd(&program).args([
            "-c",
            "import importlib.metadata; print(importlib.metadata.version('grimp'))",
        ]),
        Tool::Gsttt | Tool::DependencyCruiser | Tool::Madge | Tool::CodebaseMemory => {
            sh.cmd(&program).arg("--version")
        }
    };
    let output = cmd
        .quiet()
        .read()
        .with_context(|| format!("cannot ask {} for its version", tool.name()))?;
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .map(str::to_owned)
        .with_context(|| format!("{} printed no version", tool.name()))
}

/// Puts the repository at its pinned commit in `dir`: a shallow fetch of that commit alone, unless `dir` is already at it.
pub(super) fn checkout(dir: &Path, repository: &Repository) -> anyhow::Result<()> {
    // `.git` is looked for first: without it `rev-parse` would find the repository that holds `dir`.
    if dir.join(".git").exists() && git(dir, &["rev-parse", "HEAD"])? == repository.commit {
        return Ok(());
    }
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    git(dir, &["init", "--quiet"])?;
    git(
        dir,
        &[
            "fetch",
            "--quiet",
            "--depth",
            "1",
            repository.url.as_str(),
            repository.commit.as_str(),
        ],
    )?;
    git(
        dir,
        &["checkout", "--quiet", "--detach", "--force", "FETCH_HEAD"],
    )?;
    let head = git(dir, &["rev-parse", "HEAD"])?;
    ensure!(
        head == repository.commit,
        "{} is at {head}, not at the pinned {}",
        repository.name,
        repository.commit
    );
    Ok(())
}

/// The number of tracked files of the checkout in `dir` with an extension of the language of the repository.
pub(super) fn count_files(dir: &Path, repository: &Repository) -> anyhow::Result<u64> {
    let listing = git(dir, &["ls-files", "-z"])?;
    Ok(count_source_files(&listing, repository.language))
}

/// The files of a `git ls-files -z` listing that have an extension of `language`.
fn count_source_files(listing: &str, language: Language) -> u64 {
    let extensions = language.extensions();
    let files = listing.split('\0').filter(|file| {
        Path::new(file)
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extensions.contains(&extension))
    });
    u64::try_from(files.count()).unwrap_or(u64::MAX)
}

/// Installs dependency-cruiser, madge and the TypeScript they read with, side by side, with pnpm.
fn node_tools(programs: &Programs, pins: &Pins) -> anyhow::Result<()> {
    let packages = pins.tools.node_packages();
    if packages
        .iter()
        .all(|(name, version)| node_version(&programs.node, name).as_deref() == Some(*version))
    {
        return Ok(());
    }
    fs::create_dir_all(&programs.node)
        .with_context(|| format!("cannot create {}", programs.node.display()))?;
    let specs = packages
        .iter()
        .map(|(name, version)| format!("{name}@{version}"));
    Shell::new()?
        .cmd("pnpm")
        .args(["add", "--ignore-workspace", "--save-exact", "--dir"])
        .arg(&programs.node)
        .args(specs)
        .run()
        .context("pnpm could not install the Node tools")
}

/// The version of a package in the `node_modules` of `dir`, as the manifest of the package says.
fn node_version(dir: &Path, package: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Manifest {
        version: String,
    }
    let manifest =
        fs::read_to_string(dir.join("node_modules").join(package).join("package.json")).ok()?;
    Some(serde_json::from_str::<Manifest>(&manifest).ok()?.version)
}

/// Installs grimp into a virtual environment with uv.
fn python_tools(programs: &Programs, pins: &Pins) -> anyhow::Result<()> {
    let version = pins.tools.grimp.version.as_str();
    if installed_version(Tool::Grimp, programs).ok().as_deref() == Some(version) {
        return Ok(());
    }
    let sh = Shell::new()?;
    let python = programs.of(Tool::Grimp);
    if !python.exists() {
        sh.cmd("uv")
            .args(["venv", "--quiet"])
            .arg(&programs.venv)
            .run()
            .context("uv could not create the virtual environment of grimp")?;
    }
    sh.cmd("uv")
        .args(["pip", "install", "--quiet", "--python"])
        .arg(&python)
        .arg(format!("grimp=={version}"))
        .run()
        .context("uv could not install grimp")
}

/// Downloads codebase-memory-mcp for this platform, checks the archive against the pinned sum and unpacks it.
fn cbm(programs: &Programs, pins: &Pins) -> anyhow::Result<()> {
    let cbm = &pins.tools.codebase_memory;
    if installed_version(Tool::CodebaseMemory, programs)
        .ok()
        .as_deref()
        == Some(cbm.version.as_str())
    {
        return Ok(());
    }
    let platform = platform();
    let archive = cbm.archives.get(&platform).with_context(|| {
        format!("bench/bench.toml pins no archive of codebase-memory-mcp for {platform}")
    })?;
    fs::create_dir_all(&programs.cbm)
        .with_context(|| format!("cannot create {}", programs.cbm.display()))?;
    let file = programs.cbm.join(&archive.file);
    let url = format!("{}/v{}/{}", cbm.releases, cbm.version, archive.file);
    Shell::new()?
        .cmd("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
        ])
        .arg(&file)
        .arg(&url)
        .run()
        .with_context(|| format!("cannot download {url}"))?;
    let unpacked = unpack_verified(&file, &archive.sha256, &programs.cbm);
    // The archive is large and its binary is what is kept.
    let removed =
        fs::remove_file(&file).with_context(|| format!("cannot remove {}", file.display()));
    unpacked?;
    removed
}

/// The key of the archive for this machine in the pins: `darwin-arm64`, `linux-amd64` and so on.
fn platform() -> String {
    platform_key(std::env::consts::OS, std::env::consts::ARCH)
}

fn platform_key(os: &str, arch: &str) -> String {
    let os = if os == "macos" { "darwin" } else { os };
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// Unpacks `archive` into `destination` once its SHA-256 is `sha256`. An archive with another sum is refused before anything is unpacked.
fn unpack_verified(archive: &Path, sha256: &str, destination: &Path) -> anyhow::Result<()> {
    let actual = checksum(archive)?;
    ensure!(
        actual == sha256,
        "{} has the SHA-256 {actual}, but bench/bench.toml pins {sha256}, so nothing was unpacked",
        archive.display()
    );
    let file = File::open(archive).with_context(|| format!("cannot read {}", archive.display()))?;
    tar::Archive::new(GzDecoder::new(file))
        .unpack(destination)
        .with_context(|| {
            format!(
                "cannot unpack {} into {}",
                archive.display(),
                destination.display()
            )
        })
}

/// The SHA-256 of a file as 64 lower-case hexadecimal digits, read in pieces: an archive is too large to hold in memory twice.
fn checksum(file: &Path) -> anyhow::Result<String> {
    let input = File::open(file).with_context(|| format!("cannot read {}", file.display()))?;
    let mut reader = std::io::BufReader::new(input);
    let mut hasher = Sha256::new();
    loop {
        let piece = reader
            .fill_buf()
            .with_context(|| format!("cannot read {}", file.display()))?;
        if piece.is_empty() {
            break;
        }
        hasher.update(piece);
        let length = piece.len();
        reader.consume(length);
    }
    Ok(faster_hex::hex_string(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    use super::*;

    /// Writes a gzipped tarball with one executable in `dir` and returns its path.
    fn archive_in(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("cbm.tar.gz");
        let mut builder = tar::Builder::new(GzEncoder::new(
            File::create(&path).unwrap(),
            Compression::default(),
        ));
        let body = b"#!/bin/sh\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(body.len()).unwrap());
        header.set_mode(0o755);
        builder
            .append_data(&mut header, "codebase-memory-mcp", &body[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    #[gsttt_scenario::scenario("benchmarks", "An archive with another checksum is refused")]
    #[test]
    fn an_archive_with_another_checksum_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_in(dir.path());
        let pinned = checksum(&archive).unwrap();
        let other = "0".repeat(64);
        let destination = dir.path().join("unpacked");

        let error = unpack_verified(&archive, &other, &destination)
            .unwrap_err()
            .to_string();
        assert!(error.contains("cbm.tar.gz"), "{error}");
        assert!(error.contains(&other) && error.contains(&pinned), "{error}");
        assert!(!destination.exists(), "nothing is unpacked");

        unpack_verified(&archive, &pinned, &destination).unwrap();
        assert!(destination.join("codebase-memory-mcp").is_file());
    }

    #[test]
    fn the_checksum_is_the_sha256_in_lower_case_hexadecimal() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("abc");
        fs::write(&file, "abc").unwrap();
        assert_eq!(
            checksum(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn the_platform_is_the_key_of_the_archive() {
        assert_eq!(platform_key("macos", "aarch64"), "darwin-arm64");
        assert_eq!(platform_key("macos", "x86_64"), "darwin-amd64");
        assert_eq!(platform_key("linux", "x86_64"), "linux-amd64");
        assert_eq!(platform_key("linux", "aarch64"), "linux-arm64");
    }

    #[test]
    fn files_are_counted_by_the_extensions_of_the_language() {
        let listing =
            "src/a.ts\0src/b.tsx\0lib/c.mjs\0README.md\0docs/d.py\0src/e.rs\0package.json\0";
        assert_eq!(count_source_files(listing, Language::Typescript), 3);
        assert_eq!(count_source_files(listing, Language::Python), 1);
        assert_eq!(count_source_files(listing, Language::Rust), 1);
    }
}
