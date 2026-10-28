//! `install.sh`, run on packages made by `cargo xtask package` and served from a local directory. Programs the installer asks the network or the machine, `gh`, `curl` and `uname`, are replaced by scripts in front of `PATH`.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::support::{RELEASE_TARGETS, commit, package, printed, repository, workspace_root};

/// Answers `uname -m` and `uname -s` with `FAKE_ARCH` and `FAKE_OS`.
const UNAME: &str = r#"case "$1" in
  -m) echo "$FAKE_ARCH" ;;
  *) echo "$FAKE_OS" ;;
esac
"#;

/// A `gh` that is signed in when `FAKE_SIGNED_IN` is `yes`, whose latest release is `FAKE_LATEST` and whose downloads come from `FAKE_PACKAGES`. Every call is appended to `FAKE_LOG`.
const GH: &str = r#"echo "gh $*" >> "$FAKE_LOG"
case "$1 $2" in
  "auth status") [ "$FAKE_SIGNED_IN" = yes ] ;;
  "release view") echo "v$FAKE_LATEST" ;;
  "release download")
    patterns=""
    directory=.
    while [ $# -gt 0 ]; do
      case "$1" in
        --pattern) patterns="$patterns $2"; shift ;;
        --dir) directory="$2"; shift ;;
      esac
      shift
    done
    for pattern in $patterns; do cp "$FAKE_PACKAGES/$pattern" "$directory/$pattern"; done ;;
  *) exit 2 ;;
esac
"#;

/// A `curl` that stands in for github.com: the latest release is `FAKE_LATEST` and the downloads come from `FAKE_PACKAGES`. Every call is appended to `FAKE_LOG`.
const CURL: &str = r#"echo "curl $*" >> "$FAKE_LOG"
output=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) output="$2"; shift ;;
    -w) shift ;;
    -*) ;;
    *) url="$1" ;;
  esac
  shift
done
case "$url" in
  https://github.com/SkuratovichA/gsttt/releases/latest)
    printf '%s' "https://github.com/SkuratovichA/gsttt/releases/tag/v$FAKE_LATEST" ;;
  https://github.com/SkuratovichA/gsttt/releases/download/v*/*)
    cp "$FAKE_PACKAGES/${url##*/}" "$output" ;;
  *) echo "no such URL: $url" >&2; exit 22 ;;
esac
"#;

/// Release packages for all four targets, one version for each of `labels`: 0.1.1, 0.1.2, and so on. The `gsttt` in a package is a script that prints its label and its target.
fn releases(labels: &[&str]) -> TempDir {
    let repo = repository("0.1.0");
    let target_dir = tempfile::tempdir().unwrap();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            commit(repo.path(), "feat: the next release");
        }
        for target in RELEASE_TARGETS {
            let directory = target_dir.path().join(target).join("dist");
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("gsttt"),
                format!("#!/bin/sh\necho 'gsttt {label} {target}'\n"),
            )
            .unwrap();
            let output = package(repo.path(), target_dir.path(), target);
            assert!(output.status.success(), "{}", printed(&output));
        }
    }
    target_dir
}

fn packages(releases: &TempDir) -> PathBuf {
    releases.path().join("packages")
}

/// Every file of `directory` with its content.
fn contents(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&path).unwrap())
        })
        .collect()
}

/// A machine to install on: an empty home directory, which also holds the install directory.
struct Machine {
    home: TempDir,
}

impl Machine {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn install_dir(&self) -> PathBuf {
        self.home.path().join("bin")
    }

    /// `sh install.sh` with this machine's install directory and nothing else set.
    fn bare_installer(&self) -> Command {
        let mut command = Command::new("sh");
        command
            .arg(workspace_root().join("install.sh"))
            .current_dir(self.home.path())
            .env("HOME", self.home.path())
            .env("GSTTT_INSTALL_DIR", self.install_dir())
            .env_remove("GSTTT_VERSION")
            .env_remove("GSTTT_BASE_URL");
        command
    }

    /// The installer, taking the packages from the directory `packages`.
    fn installer(&self, packages: &Path) -> Command {
        let mut command = self.bare_installer();
        command.env("GSTTT_BASE_URL", format!("file://{}", packages.display()));
        command
    }

    /// What the installed `gsttt` prints.
    fn installed(&self) -> String {
        let output = Command::new(self.install_dir().join("gsttt"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", printed(&output));
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// The calls the scripts of `shims` logged.
    fn calls(&self) -> String {
        fs::read_to_string(self.home.path().join("calls.log")).unwrap_or_default()
    }
}

/// Puts the scripts `scripts` (name and body) in front of `PATH` of `command`. The scripts exist as long as the returned directory does.
fn shims(command: &mut Command, machine: &Machine, scripts: &[(&str, &str)]) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, body) in scripts {
        let path = directory.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let rest = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(directory.path().to_path_buf()).chain(std::env::split_paths(&rest)),
    )
    .unwrap();
    command
        .env("PATH", path)
        .env("FAKE_LOG", machine.home.path().join("calls.log"));
    directory
}

/// The installer with a `gh` and a `curl` in front of `PATH`, whose releases are the packages in `packages` and whose latest release is 0.1.2.
fn installer_with_github(
    machine: &Machine,
    packages: &Path,
    signed_in: bool,
) -> (Command, TempDir) {
    let mut command = machine.bare_installer();
    let scripts = shims(&mut command, machine, &[("gh", GH), ("curl", CURL)]);
    command
        .env("FAKE_PACKAGES", packages)
        .env("FAKE_LATEST", "0.1.2")
        .env("FAKE_SIGNED_IN", if signed_in { "yes" } else { "no" });
    (command, scripts)
}

#[gsttt_scenario::scenario("distribution", "A verified package is installed")]
#[test]
fn a_verified_package_is_installed() {
    let releases = releases(&["first"]);
    let machine = Machine::new();

    let output = machine.installer(&packages(&releases)).output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    let binary = machine.install_dir().join("gsttt");
    assert_eq!(
        fs::metadata(&binary).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(
        machine.installed().starts_with("gsttt first "),
        "{}",
        machine.installed()
    );
}

#[gsttt_scenario::scenario("distribution", "A bad checksum installs nothing")]
#[test]
fn a_bad_checksum_installs_nothing() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for (name, _) in contents(&packages) {
        if let Some(archive) = name.strip_suffix(".sha256") {
            fs::write(
                packages.join(&name),
                format!("{}  {archive}\n", "0".repeat(64)),
            )
            .unwrap();
        }
    }
    let machine = Machine::new();
    fs::create_dir_all(machine.install_dir()).unwrap();
    fs::write(machine.install_dir().join("gsttt"), "the old binary").unwrap();
    let before = contents(&machine.install_dir());

    let output = machine.installer(&packages).output().unwrap();

    assert!(!output.status.success());
    assert!(
        printed(&output).contains("nothing was installed"),
        "{}",
        printed(&output)
    );
    assert_eq!(contents(&machine.install_dir()), before);
}

#[test]
fn a_package_that_changed_after_its_checksum_was_made_installs_nothing() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for (name, _) in contents(&packages) {
        if name.ends_with(".tar.gz") {
            let mut bytes = fs::read(packages.join(&name)).unwrap();
            bytes.push(0);
            fs::write(packages.join(&name), bytes).unwrap();
        }
    }
    let machine = Machine::new();

    let output = machine.installer(&packages).output().unwrap();

    assert!(!output.status.success());
    assert!(
        !machine.install_dir().exists(),
        "a failed install made the install directory"
    );
}

#[test]
fn a_checksum_file_that_is_not_a_checksum_installs_nothing() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for (name, _) in contents(&packages) {
        if name.strip_suffix(".sha256").is_some() {
            fs::write(packages.join(name), "not a checksum\n").unwrap();
        }
    }
    let machine = Machine::new();

    let output = machine.installer(&packages).output().unwrap();

    assert!(!output.status.success());
    assert!(
        printed(&output).contains("does not start with a SHA-256"),
        "{}",
        printed(&output)
    );
    assert!(!machine.install_dir().exists());
}

#[test]
fn the_package_is_picked_for_the_operating_system_and_architecture() {
    let releases = releases(&["first"]);
    for (os, arch, target) in [
        ("Linux", "x86_64", "x86_64-unknown-linux-musl"),
        ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
        ("Darwin", "x86_64", "x86_64-apple-darwin"),
        ("Darwin", "arm64", "aarch64-apple-darwin"),
    ] {
        let machine = Machine::new();
        let mut command = machine.installer(&packages(&releases));
        let _scripts = shims(&mut command, &machine, &[("uname", UNAME)]);
        command.env("FAKE_OS", os).env("FAKE_ARCH", arch);

        let output = command.output().unwrap();

        assert!(output.status.success(), "{os} {arch}: {}", printed(&output));
        assert_eq!(machine.installed(), format!("gsttt first {target}"));
    }
}

#[test]
fn a_machine_without_a_package_is_told_so() {
    let releases = releases(&["first"]);
    for (os, arch, named) in [
        ("FreeBSD", "x86_64", "FreeBSD"),
        ("Linux", "riscv64", "riscv64"),
    ] {
        let machine = Machine::new();
        let mut command = machine.installer(&packages(&releases));
        let _scripts = shims(&mut command, &machine, &[("uname", UNAME)]);
        command.env("FAKE_OS", os).env("FAKE_ARCH", arch);

        let output = command.output().unwrap();

        assert!(!output.status.success(), "{os} {arch}");
        assert!(printed(&output).contains(named), "{}", printed(&output));
        assert!(!machine.install_dir().exists());
    }
}

#[test]
fn the_newest_version_is_installed_unless_one_is_named() {
    let releases = releases(&["first", "second"]);
    let packages = packages(&releases);

    let machine = Machine::new();
    assert!(
        machine
            .installer(&packages)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(machine.installed().starts_with("gsttt second "));

    for version in ["0.1.1", "v0.1.1"] {
        let machine = Machine::new();
        let output = machine
            .installer(&packages)
            .env("GSTTT_VERSION", version)
            .output()
            .unwrap();
        assert!(output.status.success(), "{version}: {}", printed(&output));
        assert!(machine.installed().starts_with("gsttt first "), "{version}");
    }
}

#[test]
fn a_file_named_latest_beside_the_packages_names_the_latest_version() {
    let releases = releases(&["first", "second"]);
    let packages = packages(&releases);
    fs::write(packages.join("latest"), "0.1.1\n").unwrap();
    let machine = Machine::new();

    let output = machine.installer(&packages).output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(machine.installed().starts_with("gsttt first "));
}

#[test]
fn the_binary_goes_to_the_local_bin_of_the_home_directory_by_default() {
    let releases = releases(&["first"]);
    let machine = Machine::new();

    let output = machine
        .installer(&packages(&releases))
        .env_remove("GSTTT_INSTALL_DIR")
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(machine.home.path().join(".local/bin/gsttt").is_file());
}

#[test]
fn gh_downloads_the_release_when_it_is_signed_in() {
    let releases = releases(&["first", "second"]);
    let packages = packages(&releases);
    let machine = Machine::new();
    let (mut command, _scripts) = installer_with_github(&machine, &packages, true);

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(machine.installed().starts_with("gsttt second "));
    let calls = machine.calls();
    assert!(calls.contains("gh release download v0.1.2 "), "{calls}");
    assert!(!calls.contains("curl"), "{calls}");

    let machine = Machine::new();
    let (mut command, _scripts) = installer_with_github(&machine, &packages, true);
    let output = command.env("GSTTT_VERSION", "0.1.1").output().unwrap();
    assert!(output.status.success(), "{}", printed(&output));
    assert!(machine.installed().starts_with("gsttt first "));
    assert!(!machine.calls().contains("release view"));
}

#[test]
fn curl_downloads_the_release_when_gh_is_not_signed_in() {
    let releases = releases(&["first", "second"]);
    let packages = packages(&releases);
    let machine = Machine::new();
    let (mut command, _scripts) = installer_with_github(&machine, &packages, false);

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(machine.installed().starts_with("gsttt second "));
    let calls = machine.calls();
    assert!(
        calls.contains("/releases/download/v0.1.2/gsttt-0.1.2-"),
        "{calls}"
    );
    assert!(!calls.contains("gh release"), "{calls}");
}

#[test]
fn a_base_url_replaces_the_release_even_when_gh_is_signed_in() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    let machine = Machine::new();
    let mut command = machine.installer(&packages);
    let _scripts = shims(&mut command, &machine, &[("gh", GH)]);
    command
        .env("FAKE_PACKAGES", &packages)
        .env("FAKE_LATEST", "0.1.2")
        .env("FAKE_SIGNED_IN", "yes");

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(machine.installed().starts_with("gsttt first "));
    assert!(!machine.calls().contains("gh "), "{}", machine.calls());
}
