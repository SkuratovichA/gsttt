//! `install.sh`, run on packages made by `cargo xtask package` and served from a local directory. Programs the installer asks the network or the machine, `gh`, `curl`, `uname` and `sysctl`, are replaced by scripts in front of `PATH`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::support::{
    RELEASE_TARGETS, commit, package, printed, repository, workspace_root, write_executable,
};

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
    -w | --proto | --connect-timeout | --max-time | --retry) shift ;;
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

/// A `sysctl` that answers `sysctl -n sysctl.proc_translated` with `FAKE_TRANSLATED`, and fails as an Intel Mac does when that is `none`. Every call is appended to `FAKE_LOG`.
const SYSCTL: &str = r#"echo "sysctl $*" >> "$FAKE_LOG"
if [ "$*" != "-n sysctl.proc_translated" ] || [ "$FAKE_TRANSLATED" = none ]; then
  echo "sysctl: unknown oid" >&2
  exit 1
fi
echo "$FAKE_TRANSLATED"
"#;

/// A `curl` that appends its call to `FAKE_LOG` and runs the next `curl` on `PATH`. A script from `shims` is the first on `PATH`, so dropping the first entry finds the real one.
const CURL_LOGGING: &str = r#"echo "curl $*" >> "$FAKE_LOG"
PATH="${PATH#*:}"
exec curl "$@"
"#;

/// A `cp` that ends the copy to the staged file of the installer by sending the installer `TERM`, as an interrupt would between the copy and the rename.
const CP_THEN_TERM: &str = r#"PATH="${PATH#*:}"
cp "$@" || exit 1
case "$2" in
  */.gsttt.*) kill -TERM "$PPID" ;;
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

/// The `PATH` of this test run with `directories` in front of it.
fn path_with(directories: &[&Path]) -> OsString {
    let rest = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(
        directories
            .iter()
            .map(|directory| directory.to_path_buf())
            .chain(std::env::split_paths(&rest)),
    )
    .unwrap()
}

/// Puts the scripts `scripts` (name and body) in front of `PATH` of `command`. The scripts exist as long as the returned directory does.
fn shims(command: &mut Command, machine: &Machine, scripts: &[(&str, &str)]) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, body) in scripts {
        write_executable(directory.path(), name, &format!("#!/bin/sh\n{body}"));
    }
    command
        .env("PATH", path_with(&[directory.path()]))
        .env("FAKE_LOG", machine.home.path().join("calls.log"));
    directory
}

/// The first program called `name` on the `PATH` of this test run.
fn program(name: &str) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let found = std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file());
    assert!(found.is_some(), "{name} is not on PATH");
    found.unwrap()
}

/// A directory to be the whole of `PATH`: links to the programs `install.sh` runs, and a `curl` that appends its call to `FAKE_LOG` and fails. It holds neither `sha256sum` nor `shasum`.
fn path_without_checksum_tools() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    for name in [
        "sh", "uname", "mktemp", "rm", "mkdir", "tar", "cp", "chmod", "mv", "cut", "sort", "tail",
    ] {
        std::os::unix::fs::symlink(program(name), directory.path().join(name)).unwrap();
    }
    write_executable(
        directory.path(),
        "curl",
        "#!/bin/sh\necho \"curl $*\" >> \"$FAKE_LOG\"\nexit 22\n",
    );
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

#[gsttt_scenario::scenario("distribution", "A GitHub download is over HTTPS only")]
#[test]
fn curl_fetches_github_over_https_with_a_bounded_wait() {
    let releases = releases(&["first", "second"]);
    let packages = packages(&releases);
    let machine = Machine::new();
    let (mut command, _scripts) = installer_with_github(&machine, &packages, false);

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    let calls = machine.calls();
    let curl_calls: Vec<&str> = calls
        .lines()
        .filter(|line| line.starts_with("curl "))
        .collect();
    // The lookup of the latest release, the package and its checksum.
    assert_eq!(curl_calls.len(), 3, "{calls}");
    for call in curl_calls {
        for flag in [
            "--proto =https",
            "--tlsv1.2",
            "--connect-timeout",
            "--max-time",
            "--retry",
        ] {
            assert!(call.contains(flag), "{flag} is missing from: {call}");
        }
    }
}

#[gsttt_scenario::scenario("distribution", "A base URL is fetched as given")]
#[test]
fn a_base_url_is_fetched_without_the_limits_meant_for_github() {
    let releases = releases(&["first"]);
    let machine = Machine::new();
    let mut command = machine.installer(&packages(&releases));
    let _scripts = shims(&mut command, &machine, &[("curl", CURL_LOGGING)]);

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    let calls = machine.calls();
    assert!(calls.contains("curl "), "curl fetched nothing: {calls}");
    for flag in [
        "--proto",
        "--tlsv1.2",
        "--connect-timeout",
        "--max-time",
        "--retry",
    ] {
        assert!(!calls.contains(flag), "{flag} was given to curl: {calls}");
    }
}

#[gsttt_scenario::scenario(
    "distribution",
    "A missing checksum tool stops the install before any download"
)]
#[test]
fn a_machine_without_a_checksum_tool_is_told_so_before_anything_is_downloaded() {
    let releases = releases(&["first"]);
    let machine = Machine::new();
    let path = path_without_checksum_tools();
    let mut command = machine.installer(&packages(&releases));
    command
        .env("PATH", path.path())
        .env("FAKE_LOG", machine.home.path().join("calls.log"));

    let output = command.output().unwrap();

    assert!(!output.status.success(), "{}", printed(&output));
    assert!(
        printed(&output).contains("sha256sum or shasum"),
        "{}",
        printed(&output)
    );
    assert_eq!(machine.calls(), "", "something was downloaded");
    assert!(!machine.install_dir().exists());
}

#[gsttt_scenario::scenario("distribution", "A malformed version is refused")]
#[test]
fn a_version_that_is_not_numbers_joined_by_single_dots_is_refused() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for version in [".", "..", "1..2", ".1.2", "1.2.", "1.2.x", "1/2"] {
        let machine = Machine::new();
        let mut command = machine.installer(&packages);
        let _scripts = shims(&mut command, &machine, &[("curl", CURL_LOGGING)]);

        let output = command.env("GSTTT_VERSION", version).output().unwrap();

        assert!(!output.status.success(), "{version}: {}", printed(&output));
        assert!(
            printed(&output).contains("cannot tell which version"),
            "{version}: {}",
            printed(&output)
        );
        assert_eq!(machine.calls(), "", "{version}: something was downloaded");
        assert!(!machine.install_dir().exists(), "{version}");
    }
}

#[gsttt_scenario::scenario("distribution", "A shell under Rosetta gets the Apple Silicon package")]
#[test]
fn a_shell_under_rosetta_gets_the_apple_silicon_package() {
    let releases = releases(&["first"]);
    for (os, arch, translated, target) in [
        ("Darwin", "x86_64", "1", "aarch64-apple-darwin"),
        ("Darwin", "x86_64", "0", "x86_64-apple-darwin"),
        ("Darwin", "x86_64", "none", "x86_64-apple-darwin"),
        ("Darwin", "arm64", "0", "aarch64-apple-darwin"),
        ("Linux", "x86_64", "1", "x86_64-unknown-linux-musl"),
    ] {
        let machine = Machine::new();
        let mut command = machine.installer(&packages(&releases));
        let _scripts = shims(
            &mut command,
            &machine,
            &[("uname", UNAME), ("sysctl", SYSCTL)],
        );
        command
            .env("FAKE_OS", os)
            .env("FAKE_ARCH", arch)
            .env("FAKE_TRANSLATED", translated);

        let output = command.output().unwrap();

        let case = format!("{os} {arch}, translated {translated}");
        assert!(output.status.success(), "{case}: {}", printed(&output));
        assert_eq!(
            machine.installed(),
            format!("gsttt first {target}"),
            "{case}"
        );
    }
}

#[gsttt_scenario::scenario("distribution", "A directory on PATH gets no hint")]
#[test]
fn a_directory_on_the_path_gets_no_hint_however_it_is_spelled() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for (given, listed) in [("", ""), ("/", ""), ("", "/")] {
        let machine = Machine::new();
        let directory = machine.install_dir().display().to_string();
        let mut command = machine.installer(&packages);
        command
            .env("GSTTT_INSTALL_DIR", format!("{directory}{given}"))
            .env(
                "PATH",
                path_with(&[Path::new(&format!("{directory}{listed}"))]),
            );

        let output = command.output().unwrap();

        let case = format!("installed into {directory}{given}, PATH lists {directory}{listed}");
        assert!(output.status.success(), "{case}: {}", printed(&output));
        for hint in ["your PATH", "export PATH="] {
            assert!(
                !printed(&output).contains(hint),
                "{case}: {}",
                printed(&output)
            );
        }
    }
}

#[gsttt_scenario::scenario("distribution", "A directory off PATH gets the line to add")]
#[test]
fn a_directory_off_the_path_comes_with_the_line_that_puts_it_there() {
    let releases = releases(&["first"]);
    let machine = Machine::new();

    let output = machine.installer(&packages(&releases)).output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    let line = format!("export PATH=\"{}:$PATH\"", machine.install_dir().display());
    assert!(printed(&output).contains(&line), "{}", printed(&output));
}

/// Puts a `gsttt` in the install directory of `machine` that prints `version` whatever it is asked.
fn install_fake_gsttt(machine: &Machine, version: &str) {
    write_executable(
        &machine.install_dir(),
        "gsttt",
        &format!("#!/bin/sh\necho '{version}'\n"),
    );
}

#[gsttt_scenario::scenario("distribution", "An installed version is not downloaded again")]
#[test]
fn a_version_that_is_installed_is_not_downloaded_again() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    fs::write(packages.join("latest"), "0.1.1\n").unwrap();
    let machine = Machine::new();
    install_fake_gsttt(&machine, "gsttt 0.1.1");
    let before = contents(&machine.install_dir());
    let mut command = machine.installer(&packages);
    let _scripts = shims(&mut command, &machine, &[("curl", CURL_LOGGING)]);

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    let binary = machine.install_dir().join("gsttt");
    let said = format!("gsttt 0.1.1 is already installed at {}", binary.display());
    assert!(printed(&output).contains(&said), "{}", printed(&output));
    let hint = format!("export PATH=\"{}:$PATH\"", machine.install_dir().display());
    assert!(printed(&output).contains(&hint), "{}", printed(&output));
    let calls = machine.calls();
    assert_eq!(calls.lines().count(), 1, "{calls}");
    assert!(calls.trim_end().ends_with("/latest"), "{calls}");
    assert_eq!(contents(&machine.install_dir()), before);
}

#[test]
fn a_gsttt_of_another_version_is_replaced() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for installed in ["gsttt 0.1.0", "gsttt 0.1.10", "gsttt 0.1.1-dev", "0.1.1"] {
        let machine = Machine::new();
        install_fake_gsttt(&machine, installed);

        let output = machine.installer(&packages).output().unwrap();

        assert!(output.status.success(), "{installed}: {}", printed(&output));
        assert!(
            machine.installed().starts_with("gsttt first "),
            "{installed}"
        );
        assert!(
            !printed(&output).contains("already installed"),
            "{installed}: {}",
            printed(&output)
        );
    }
}

#[gsttt_scenario::scenario("distribution", "An interrupted install leaves nothing behind")]
#[test]
fn an_install_interrupted_after_the_copy_leaves_nothing_behind() {
    let releases = releases(&["first"]);
    let machine = Machine::new();
    let temporary = machine.home.path().join("tmp");
    fs::create_dir(&temporary).unwrap();
    let mut command = machine.installer(&packages(&releases));
    let _scripts = shims(&mut command, &machine, &[("cp", CP_THEN_TERM)]);
    command.env("TMPDIR", &temporary);

    let output = command.output().unwrap();

    assert!(!output.status.success(), "{}", printed(&output));
    assert_eq!(
        contents(&machine.install_dir()),
        BTreeMap::new(),
        "the install directory is not empty"
    );
    assert_eq!(
        fs::read_dir(&temporary).unwrap().count(),
        0,
        "the download directory is left in TMPDIR"
    );
}

#[test]
fn the_download_directory_is_removed_whether_the_install_works_or_not() {
    let releases = releases(&["first"]);
    let packages = packages(&releases);
    for works in [true, false] {
        if !works {
            for (name, _) in contents(&packages) {
                if name.ends_with(".sha256") {
                    fs::write(packages.join(&name), format!("{}  x\n", "0".repeat(64))).unwrap();
                }
            }
        }
        let machine = Machine::new();
        let temporary = machine.home.path().join("tmp");
        fs::create_dir(&temporary).unwrap();

        let output = machine
            .installer(&packages)
            .env("TMPDIR", &temporary)
            .output()
            .unwrap();

        assert_eq!(output.status.success(), works, "{}", printed(&output));
        assert_eq!(
            fs::read_dir(&temporary).unwrap().count(),
            0,
            "the download directory is left in TMPDIR, the install worked: {works}"
        );
    }
}

#[gsttt_scenario::scenario("distribution", "The wrapper finds the default install")]
#[test]
fn the_wrapper_finds_what_the_installer_installs_by_default() {
    let releases = releases(&["first"]);
    let machine = Machine::new();
    let installed = machine
        .installer(&packages(&releases))
        .env_remove("GSTTT_INSTALL_DIR")
        .output()
        .unwrap();
    assert!(installed.status.success(), "{}", printed(&installed));
    let nowhere = tempfile::tempdir().unwrap();

    let output = Command::new(workspace_root().join("plugin/scripts/gsttt.sh"))
        .env_clear()
        .env("PATH", nowhere.path())
        .env("HOME", machine.home.path())
        .output()
        .unwrap();

    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("gsttt first "),
        "{}",
        printed(&output)
    );
}
