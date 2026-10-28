//! The container image: the `Dockerfile`, and the step of the `publish` job that pushes it.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;

use serde_json::json;
use tempfile::TempDir;

use crate::ci::{publish_script, run_publish_step, step, workflow};
use crate::support::{git, package, printed, repository, workspace_root, write_executable, xtask};

/// The step of the `publish` job that pushes the image.
const STEP: &str = "Container image";

/// The image the step pushes when it runs for the repository `SkuratovichA/gsttt`: the registry wants the name in lower case.
const IMAGE: &str = "ghcr.io/skuratovicha/gsttt";

/// The token the step runs with, which `docker login` must read on standard input and no other command may be given.
const TOKEN: &str = "s3cret-registry-token";

/// The Linux targets of a release, and the architecture of the image each is packed into.
const PLATFORMS: [(&str, &str); 2] = [
    ("x86_64-unknown-linux-musl", "amd64"),
    ("aarch64-unknown-linux-musl", "arm64"),
];

/// A `docker` that writes its arguments down. It reports an image as published when `IMAGE_EXISTS` is set, and it writes down what `login` read on standard input and what `buildx build` finds in its context.
const DOCKER: &str = r#"#!/bin/sh
echo "$*" >> "$DOCKER_LOG"
case "$1 $2" in
  "login ghcr.io") echo "stdin: $(cat)" >> "$DOCKER_LOG" ;;
  "buildx imagetools") test -n "$IMAGE_EXISTS" ;;
  "buildx build")
    echo "context: $(find image -type f | sort | tr '\n' ' ')" >> "$DOCKER_LOG"
    for file in image/*/gsttt; do echo "$file: $(cat "$file")" >> "$DOCKER_LOG"; done ;;
esac
"#;

/// What the publish step did: how it ended, and the lines the `docker` stub wrote.
struct Published {
    output: Output,
    docker: Vec<String>,
}

impl Published {
    /// The arguments of the `docker buildx build` the step ran, if it ran one.
    fn build(&self) -> Option<&str> {
        self.docker
            .iter()
            .map(String::as_str)
            .find(|call| call.starts_with("buildx build "))
    }
}

/// A repository with the packages of both Linux targets in `packages/`, as the build jobs hand them to the publish job, and the version they are named after. Each binary says which target it was built for.
fn with_packages() -> (TempDir, String) {
    let repo = repository("0.1.0");
    let version = String::from_utf8(xtask(repo.path(), &["version"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    let built = tempfile::tempdir().unwrap();
    for (target, _) in PLATFORMS {
        let directory = built.path().join(target).join("dist");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("gsttt"), format!("binary for {target}")).unwrap();
        let output = package(repo.path(), built.path(), target);
        assert!(output.status.success(), "{}", printed(&output));
    }
    let packages = repo.path().join("packages");
    std::fs::create_dir(&packages).unwrap();
    for entry in std::fs::read_dir(built.path().join("packages")).unwrap() {
        let file = entry.unwrap().path();
        std::fs::copy(&file, packages.join(file.file_name().unwrap())).unwrap();
    }
    (repo, version)
}

/// Runs the publish step for `version` in `repo`, with a `docker` that finds the image of that version already published when `published` says so.
fn run_image(repo: &Path, version: &str, published: bool) -> Published {
    let tools = tempfile::tempdir().unwrap();
    let log = tools.path().join("docker.log");
    write_executable(tools.path(), "docker", DOCKER);
    let sha = git(repo, &["rev-parse", "HEAD"]);
    let mut env = vec![
        ("DOCKER_LOG", log.to_str().unwrap()),
        ("GHCR_TOKEN", TOKEN),
        ("VERSION", version),
        ("GITHUB_ACTOR", "SkuratovichA"),
        ("GITHUB_REPOSITORY", "SkuratovichA/gsttt"),
        ("GITHUB_SHA", sha.as_str()),
    ];
    if published {
        env.push(("IMAGE_EXISTS", "1"));
    }
    let output = run_publish_step(repo, STEP, tools.path(), &env);
    let docker = std::fs::read_to_string(&log)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    Published { output, docker }
}

/// Asserts that the step ended well.
fn succeeded(published: &Published) {
    assert!(
        published.output.status.success(),
        "{}",
        printed(&published.output)
    );
}

/// The `Dockerfile` of the repository.
fn dockerfile() -> String {
    std::fs::read_to_string(workspace_root().join("Dockerfile")).unwrap()
}

/// The instructions of a Dockerfile: its lines that are neither empty nor comments. Each is one line, which is checked, because a line that ends in a backslash would not be read whole.
fn instructions(dockerfile: &str) -> Vec<&str> {
    let lines: Vec<&str> = dockerfile
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert!(
        lines.iter().all(|line| !line.ends_with('\\')),
        "an instruction is one line"
    );
    lines
}

/// The labels of a Dockerfile as key and value, the value without its quotes.
fn labels(dockerfile: &str) -> Vec<(String, String)> {
    instructions(dockerfile)
        .into_iter()
        .filter_map(|line| line.strip_prefix("LABEL "))
        .map(|label| {
            let (key, value) = label.split_once('=').unwrap();
            (key.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect()
}

/// The annotations of the index that the `--annotation "index:<key>=<value>"` lines of `script` set, as key and value.
fn annotations(script: &str) -> Vec<(String, String)> {
    script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("--annotation \"index:"))
        .map(|annotation| {
            let annotation = annotation
                .trim_end_matches('\\')
                .trim_end()
                .trim_end_matches('"');
            let (key, value) = annotation.split_once('=').unwrap();
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

/// The keys of `pairs`.
fn keys(pairs: &[(String, String)]) -> BTreeSet<&str> {
    pairs.iter().map(|(key, _)| key.as_str()).collect()
}

/// The lines of `script` that decide whether the release is the latest: the highest tag, and what is made of it.
fn latest_rule(script: &str) -> Vec<&str> {
    script
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("latest=") || line.starts_with("highest=") || line.contains("$highest")
        })
        .collect()
}

/// Whether `reference` names an image by a tag and the digest of its index: `<name>:<tag>@sha256:<64 hexadecimal digits>`.
fn pinned(reference: &str) -> bool {
    let Some((tagged, digest)) = reference.split_once('@') else {
        return false;
    };
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    let tag_is_there = tagged
        .rsplit_once(':')
        .is_some_and(|(name, tag)| !name.is_empty() && !tag.is_empty() && !tag.contains('/'));
    tag_is_there && hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[gsttt_scenario::scenario("distribution", "The image is built from the checked packages")]
#[test]
fn the_image_is_built_from_the_checked_packages_of_the_release() {
    let (repo, version) = with_packages();

    let published = run_image(repo.path(), &version, false);

    succeeded(&published);
    let docker = &published.docker;
    assert_eq!(
        docker[0],
        "login ghcr.io --username SkuratovichA --password-stdin"
    );
    assert_eq!(docker[1], format!("stdin: {TOKEN}"));
    assert!(
        docker[2..].iter().all(|call| !call.contains(TOKEN)),
        "the token is given to no command but docker login: {docker:?}"
    );
    let build = published.build().expect("the step builds the image");
    for part in [
        "--platform linux/amd64,linux/arm64",
        "--push",
        &format!("--tag {IMAGE}:{version}"),
    ] {
        assert!(build.contains(part), "`{part}` is not in `{build}`");
    }
    assert!(
        build.ends_with(" image"),
        "the context is the directory of the binaries, not the checkout: {build}"
    );
    assert!(
        docker.contains(&"context: image/amd64/gsttt image/arm64/gsttt ".to_owned()),
        "the context holds the binaries and nothing else: {docker:?}"
    );
    for (target, architecture) in PLATFORMS {
        let binary = format!("image/{architecture}/gsttt: binary for {target}");
        assert!(docker.contains(&binary), "`{binary}` is not in {docker:?}");
    }
}

#[gsttt_scenario::scenario(
    "distribution",
    "A package that does not match its checksum is not published"
)]
#[test]
fn a_package_that_does_not_match_its_checksum_is_not_published() {
    let (repo, version) = with_packages();
    let archive = repo
        .path()
        .join("packages")
        .join(format!("gsttt-{version}-aarch64-unknown-linux-musl.tar.gz"));
    let mut bytes = std::fs::read(&archive).unwrap();
    bytes.push(0);
    std::fs::write(&archive, bytes).unwrap();

    let published = run_image(repo.path(), &version, false);

    let printed = printed(&published.output);
    assert!(!published.output.status.success(), "{printed}");
    assert!(printed.contains("FAILED"), "{printed}");
    assert_eq!(published.build(), None);
}

#[gsttt_scenario::scenario("distribution", "Only the highest version is the latest image")]
#[test]
fn a_version_below_a_release_is_not_the_latest_image() {
    let (repo, version) = with_packages();
    git(repo.path(), &["tag", "v0.1.9"]);

    let published = run_image(repo.path(), &version, false);

    succeeded(&published);
    let build = published.build().unwrap();
    assert!(
        build.contains(&format!("--tag {IMAGE}:{version}")),
        "{build}"
    );
    assert!(!build.contains(":latest"), "{build}");
}

#[test]
fn the_highest_version_is_the_latest_image() {
    let (repo, version) = with_packages();
    git(repo.path(), &["tag", "v0.0.9"]);

    let published = run_image(repo.path(), &version, false);

    succeeded(&published);
    let build = published.build().unwrap();
    assert!(
        build.contains(&format!("--tag {IMAGE}:{version}")),
        "{build}"
    );
    assert!(build.contains(&format!("--tag {IMAGE}:latest")), "{build}");
}

/// The other copy of this rule is in the `Release` step, which a test runs as it stands, so the two are pinned against each other here.
#[test]
fn the_image_decides_the_latest_by_the_rule_of_the_release() {
    let release = publish_script("Release");
    let image = publish_script(STEP);
    let rule = latest_rule(&release);

    assert_eq!(rule.len(), 3, "the rule is three lines: {rule:?}");
    assert_eq!(
        latest_rule(&image),
        rule,
        "the image step and the Release step decide the latest differently"
    );
}

#[gsttt_scenario::scenario("distribution", "A version with an image is not pushed again")]
#[test]
fn a_version_with_an_image_is_not_pushed_again() {
    let (repo, version) = with_packages();

    let published = run_image(repo.path(), &version, true);

    succeeded(&published);
    let docker = &published.docker;
    assert!(
        docker.contains(&format!("buildx imagetools inspect {IMAGE}:{version}")),
        "the step looks for the image of the version: {docker:?}"
    );
    assert!(
        docker
            .iter()
            .all(|call| !call.starts_with("buildx create") && !call.starts_with("buildx build")),
        "{docker:?}"
    );
}

#[gsttt_scenario::scenario(
    "distribution",
    "A run after the release was made still publishes the image"
)]
#[test]
fn a_run_after_the_release_was_made_still_publishes_the_image() {
    let (repo, version) = with_packages();
    git(repo.path(), &["tag", &format!("v{version}")]);

    let published = run_image(repo.path(), &version, false);

    succeeded(&published);
    let build = published.build().expect("the step builds the image");
    assert!(
        build.contains(&format!("--tag {IMAGE}:{version}")),
        "{build}"
    );
    assert!(build.contains(&format!("--tag {IMAGE}:latest")), "{build}");
}

#[test]
fn the_image_follows_the_release_and_the_job_may_write_packages() {
    let workflow = workflow();
    let publish = &workflow["jobs"]["publish"];

    assert_eq!(
        publish["permissions"],
        json!({ "contents": "write", "packages": "write" })
    );
    let position = |name: &str| {
        publish["steps"]
            .as_array()
            .unwrap()
            .iter()
            .position(|step| step["name"] == name)
    };
    assert!(
        position(STEP) > position("Release"),
        "the image follows the release"
    );
    let image = step(publish, STEP).unwrap();
    assert_eq!(
        image["env"],
        json!({
            "GHCR_TOKEN": "${{ github.token }}",
            "VERSION": "${{ steps.version.outputs.version }}"
        })
    );
    assert!(
        image.get("if").is_none(),
        "a run for a release that was made already gets here"
    );
}

#[gsttt_scenario::scenario("distribution", "The image holds only the binary")]
#[test]
fn the_image_holds_only_the_binary() {
    let dockerfile = dockerfile();
    assert!(
        dockerfile.starts_with('#'),
        "the first line says what the file is for"
    );

    let instructions = instructions(&dockerfile);
    let keywords: Vec<&str> = instructions
        .iter()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for keyword in &keywords {
        assert!(
            ["FROM", "ARG", "LABEL", "COPY", "WORKDIR", "ENTRYPOINT"].contains(keyword),
            "`{keyword}` adds more than the binary to the image"
        );
    }
    for once in ["FROM", "COPY"] {
        assert_eq!(
            keywords.iter().filter(|keyword| **keyword == once).count(),
            1,
            "{once}"
        );
    }
    assert!(
        instructions[0].starts_with("FROM gcr.io/distroless/static-debian"),
        "{}",
        instructions[0]
    );
    for instruction in [
        "COPY --chmod=0755 ${TARGETARCH}/gsttt /usr/local/bin/gsttt",
        "WORKDIR /repo",
        r#"ENTRYPOINT ["/usr/local/bin/gsttt"]"#,
    ] {
        assert!(
            instructions.contains(&instruction),
            "the Dockerfile has no `{instruction}`"
        );
    }
}

/// The other copy of the labels is in the `--annotation` lines of the publish step, so the two are pinned against each other here.
#[gsttt_scenario::scenario("distribution", "The index repeats the labels")]
#[test]
fn the_index_repeats_the_labels_of_the_image() {
    let dockerfile = dockerfile();
    let script = publish_script(STEP);
    let labels = labels(&dockerfile);
    let annotations = annotations(&script);

    assert_eq!(labels.len(), 5, "{labels:?}");
    assert_eq!(keys(&annotations), keys(&labels));
    for (key, value) in labels.iter().filter(|(_, value)| !value.contains('$')) {
        assert!(
            annotations.contains(&(key.clone(), value.clone())),
            "the index is not annotated with {key}={value}: {annotations:?}"
        );
    }
    // A label made of a build argument is empty unless the build is given the argument.
    for name in instructions(&dockerfile)
        .iter()
        .filter_map(|line| line.strip_prefix("ARG "))
        .filter(|name| *name != "TARGETARCH")
    {
        assert!(
            script.contains(&format!("--build-arg \"{name}=")),
            "the build is not given {name}"
        );
    }
}

#[gsttt_scenario::scenario("distribution", "The base and the builder are pinned by digest")]
#[test]
fn the_base_and_the_builder_are_pinned_by_digest() {
    let dockerfile = dockerfile();
    let base = instructions(&dockerfile)
        .into_iter()
        .find_map(|line| line.strip_prefix("FROM "))
        .unwrap();
    assert!(pinned(base), "the base is `{base}`");

    let script = publish_script(STEP);
    let builder = script
        .split_whitespace()
        .skip_while(|word| *word != "--driver-opt")
        .nth(1)
        .and_then(|option| option.strip_prefix("image="))
        .expect("the builder is given its image");
    assert!(pinned(builder), "the builder is `{builder}`");
}
