//! The spec gate on small repository trees. Each test writes the files it needs into a temporary
//! directory and runs the gate on them, so nothing depends on this repository's own specs.

mod reach;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::workspace::Workspace;
use super::{Mode, Outcome, execute, mode, run};

/// The tasks of a change that is finished.
const TICKED: &str = "- [x] 1.1 Build it\n";
/// The tasks of a change with work left.
const OPEN: &str = "- [x] 1.1 Design it\n- [ ] 1.2 Build it\n";

/// The manifest of the package of every tree: a feature called `slow`, and the targets cargo finds.
const PACKAGE: &str =
    "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[features]\nslow = []\n";

/// A repository in a temporary directory with a real cargo workspace, whose targets the gate asks
/// cargo for.
struct Tree {
    dir: tempfile::TempDir,
}

impl Tree {
    /// A workspace with one member, `crates/app`, whose library is empty.
    fn new() -> Self {
        Self::with_manifest("")
    }

    /// The same, with `manifest` added to the manifest of the member: targets, mostly.
    fn with_manifest(manifest: &str) -> Self {
        let tree = Self::empty();
        tree.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/app\"]\n",
        )
        .write("crates/app/Cargo.toml", &format!("{PACKAGE}{manifest}"))
        .source("src/lib.rs", "");
        tree
    }

    /// A workspace whose only member is the package at its root, so that a walk of the package
    /// takes in the whole tree. Its files are written with [`Tree::write`].
    fn rooted() -> Self {
        let tree = Self::empty();
        tree.write("Cargo.toml", &format!("{PACKAGE}\n[workspace]\n"))
            .write("src/lib.rs", "");
        tree
    }

    fn empty() -> Self {
        let tree = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(tree.dir.path().join("openspec")).unwrap();
        tree
    }

    fn write(&self, path: &str, text: &str) -> &Self {
        let file = self.dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
        self
    }

    /// The main spec of a capability.
    fn spec(&self, capability: &str, text: &str) -> &Self {
        self.write(&format!("openspec/specs/{capability}/spec.md"), text)
    }

    /// An open change with its task list and one delta per capability.
    fn change(&self, name: &str, tasks: &str, deltas: &[(&str, &str)]) -> &Self {
        self.write(&format!("openspec/changes/{name}/tasks.md"), tasks);
        for (capability, text) in deltas {
            self.write(
                &format!("openspec/changes/{name}/specs/{capability}/spec.md"),
                text,
            );
        }
        self
    }

    /// A source file of the member.
    fn source(&self, file: &str, text: &str) -> &Self {
        self.write(&format!("crates/app/{file}"), text)
    }

    fn workspace(&self) -> Workspace {
        Workspace::load(self.dir.path()).unwrap()
    }

    fn try_run(&self, mode: Mode) -> anyhow::Result<Outcome> {
        execute(&Workspace::load(self.dir.path())?, mode)
    }

    fn gate(&self) -> Outcome {
        self.try_run(Mode::Report).unwrap()
    }
}

/// A main spec with the given requirements.
fn main_spec(requirements: &[String]) -> String {
    format!(
        "# Queries Specification\n\n## Purpose\nA purpose.\n\n## Requirements\n\n{}",
        requirements.join("\n")
    )
}

/// A delta with one section: `ADDED`, `MODIFIED` or `REMOVED`.
fn delta(section: &str, requirements: &[String]) -> String {
    format!(
        "# Spec Delta\n\n## {section} Requirements\n\n{}",
        requirements.join("\n")
    )
}

/// A requirement with one scenario for each title.
fn requirement(name: &str, titles: &[&str]) -> String {
    let scenarios: Vec<String> = titles
        .iter()
        .map(|title| {
            format!(
                "#### Scenario: {title}\n- **WHEN** something happens\n- **THEN** something follows\n"
            )
        })
        .collect();
    format!(
        "### Requirement: {name}\nThe system SHALL do it.\n\n{}",
        scenarios.join("\n")
    )
}

/// A requirement as a `REMOVED` section lists it.
fn removal(name: &str) -> String {
    format!("### Requirement: {name}\n**Reason**: No longer needed.\n**Migration**: None.\n")
}

/// The `RENAMED` entry for one requirement.
fn renaming(from: &str, to: &str) -> String {
    format!("- FROM: `### Requirement: {from}`\n- TO: `### Requirement: {to}`\n")
}

/// A test that claims a scenario; its function starts on line 3.
fn proof(capability: &str, scenario: &str) -> String {
    format!("#[scenario(\"{capability}\", \"{scenario}\")]\n#[test]\nfn proof() {{}}\n")
}

/// The line of the first line of `text` that contains `needle`.
fn line_of(text: &str, needle: &str) -> usize {
    text.lines().position(|line| line.contains(needle)).unwrap() + 1
}

/// The lines of all lines of `text` that contain `needle`.
fn lines_of(text: &str, needle: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(index, _)| index + 1)
        .collect()
}

#[gsttt_scenario::scenario("spec-gate", "Proofs inside proptest count")]
#[test]
fn proofs_inside_proptest_count() {
    let source = r#"use proptest::prelude::*;

proptest! {
    #[test]
    #[scenario("queries", "HTTP matching is symmetric")]
    fn symmetric(a in 0..10u8, b in 0..10u8) {
        prop_assert_eq!(a.max(b), b.max(a));
    }
}
"#;
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Matching", &["HTTP matching is symmetric"])]),
    )
    .source("tests/matching.rs", source);
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("proven   HTTP matching is symmetric"),
        "{}",
        outcome.output
    );
    let proof = format!(
        "crates/app/tests/matching.rs:{} `symmetric`",
        line_of(source, "fn symmetric")
    );
    assert!(outcome.output.contains(&proof), "{}", outcome.output);
}

#[test]
fn a_proptest_function_needs_no_test_attribute() {
    let source = "proptest! {\n    #![proptest_config(ProptestConfig::default())]\n    /// Documented.\n    #[scenario(\"queries\", \"HTTP matching is symmetric\")]\n    pub fn symmetric(a in 0..10u8) { prop_assert!(a < 10); }\n}\n";
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Matching", &["HTTP matching is symmetric"])]),
    )
    .source("tests/matching.rs", source);
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains("1 proven, 0 pending, 0 failing"),
        "{}",
        outcome.output
    );
}

#[test]
fn any_path_ending_in_test_is_a_test_attribute() {
    let source =
        "#[scenario(\"queries\", \"Defaults apply\")]\n#[tokio::test]\nasync fn proof() {}\n";
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Defaults", &["Defaults apply"])]),
    )
    .source("tests/defaults.rs", source);
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "A marked helper is no proof")]
#[test]
fn a_marked_helper_is_no_proof() {
    let source = "#[scenario(\"queries\", \"Impact goes through wormholes\")]\nfn helper() {}\n";
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Wormholes", &["Impact goes through wormholes"])]),
    )
    .source("tests/helper.rs", source);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains(
            "crates/app/tests/helper.rs:2: `helper` is marked with #[scenario] but has no #[test] attribute"
        ),
        "{}",
        outcome.output
    );
    assert!(
        outcome
            .output
            .contains("failing  Impact goes through wormholes"),
        "{}",
        outcome.output
    );
}

#[gsttt_scenario::scenario("spec-gate", "A scenario without a proof fails")]
#[test]
fn a_scenario_without_a_proof_fails() {
    let spec = main_spec(&[requirement("Defaults", &["Defaults apply"])]);
    let tree = Tree::new();
    tree.spec("queries", &spec);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    let failure = format!(
        "openspec/specs/queries/spec.md:{}: scenario `Defaults apply` of capability `queries` has no proof",
        line_of(&spec, "Scenario: Defaults apply")
    );
    assert!(outcome.output.contains(&failure), "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "A proof of nothing fails")]
#[test]
fn a_proof_of_nothing_fails() {
    let ghost = proof("queries", "Impact goes through wormholes");
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Defaults", &["Defaults apply"])]),
    )
    .source("tests/defaults.rs", &proof("queries", "Defaults apply"))
    .source("tests/ghost.rs", &ghost);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    let failure = format!(
        "crates/app/tests/ghost.rs:{}: `proof` claims scenario `Impact goes through wormholes` of capability `queries`, but no spec or open change has it",
        line_of(&ghost, "fn proof")
    );
    assert!(outcome.output.contains(&failure), "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "A scenario proven twice fails")]
#[test]
fn a_scenario_proven_twice_fails() {
    let claim = proof("queries", "Defaults apply");
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Defaults", &["Defaults apply"])]),
    )
    .source("tests/one.rs", &claim)
    .source("tests/two.rs", &claim);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("has 2 proofs but needs exactly one"),
        "{}",
        outcome.output
    );
    let line = line_of(&claim, "fn proof");
    for file in ["one", "two"] {
        let place = format!("crates/app/tests/{file}.rs:{line} `proof`");
        assert!(outcome.output.contains(&place), "{}", outcome.output);
    }
}

#[gsttt_scenario::scenario("spec-gate", "Two scenarios with one title fail")]
#[test]
fn two_scenarios_with_one_title_fail() {
    let spec = main_spec(&[
        requirement("First", &["Defaults apply"]),
        requirement("Second", &["Defaults apply"]),
    ]);
    let tree = Tree::new();
    tree.spec("queries", &spec)
        .source("tests/defaults.rs", &proof("queries", "Defaults apply"));
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    let lines = lines_of(&spec, "Scenario: Defaults apply");
    assert_eq!(lines.len(), 2);
    let failure = format!(
        "capability `queries` has 2 scenarios titled `Defaults apply`: openspec/specs/queries/spec.md:{}, openspec/specs/queries/spec.md:{}",
        lines[0], lines[1]
    );
    assert!(outcome.output.contains(&failure), "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "Removed scenarios may not be claimed")]
#[test]
fn removed_scenarios_may_not_be_claimed() {
    let claim = proof("queries", "Impact goes through wormholes");
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Wormholes", &["Impact goes through wormholes"])]),
    )
    .change(
        "drop-wormholes",
        OPEN,
        &[("queries", &delta("REMOVED", &[removal("Wormholes")]))],
    )
    .source("tests/wormholes.rs", &claim);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    let failure = format!(
        "crates/app/tests/wormholes.rs:{}: `proof` claims scenario `Impact goes through wormholes` of capability `queries`, but change `drop-wormholes` removes its requirement `Wormholes`",
        line_of(&claim, "fn proof")
    );
    assert!(outcome.output.contains(&failure), "{}", outcome.output);

    // Without the claim the removal is clean, and the removed scenario needs no proof.
    fs::remove_file(tree.dir.path().join("crates/app/tests/wormholes.rs")).unwrap();
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "An open change is pending")]
#[test]
fn an_open_change_is_pending() {
    let tree = Tree::new();
    tree.change(
        "add-wormholes",
        OPEN,
        &[(
            "queries",
            &delta(
                "ADDED",
                &[requirement("Wormholes", &["Impact goes through wormholes"])],
            ),
        )],
    );
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains(
            "pending  Impact goes through wormholes  (openspec/changes/add-wormholes/specs/queries/spec.md:"
        ),
        "{}",
        outcome.output
    );
    assert!(
        outcome
            .output
            .contains("change `add-wormholes` still has unticked tasks"),
        "{}",
        outcome.output
    );
}

#[gsttt_scenario::scenario("spec-gate", "A change without tasks is pending")]
#[test]
fn a_change_without_tasks_is_pending() {
    let tree = Tree::new();
    // One change has a `tasks.md` with no checkbox in it, the other has no `tasks.md` at all.
    tree.change(
        "no-checkboxes",
        "# Tasks\n\nNothing is planned yet.\n",
        &[(
            "queries",
            &delta("ADDED", &[requirement("Wormholes", &["Wormholes exist"])]),
        )],
    )
    .write(
        "openspec/changes/no-file/specs/queries/spec.md",
        &delta("ADDED", &[requirement("Portals", &["Portals open"])]),
    );
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    for (scenario, change) in [
        ("Wormholes exist", "no-checkboxes"),
        ("Portals open", "no-file"),
    ] {
        let line =
            format!("pending  {scenario}  (openspec/changes/{change}/specs/queries/spec.md:");
        let found = outcome.output.lines().find(|found| found.contains(&line));
        assert!(
            found.is_some_and(|found| found.ends_with(&format!("change `{change}` has no tasks"))),
            "{}",
            outcome.output
        );
    }
}

#[gsttt_scenario::scenario("spec-gate", "A finished change must be proven")]
#[test]
fn a_finished_change_must_be_proven() {
    let added = delta(
        "ADDED",
        &[requirement("Wormholes", &["Impact goes through wormholes"])],
    );
    let tree = Tree::new();
    tree.change("add-wormholes", TICKED, &[("queries", &added)]);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    let failure = format!(
        "openspec/changes/add-wormholes/specs/queries/spec.md:{}: scenario `Impact goes through wormholes` of capability `queries` has no proof, and every task of change `add-wormholes` is ticked",
        line_of(&added, "Scenario: Impact goes through wormholes")
    );
    assert!(outcome.output.contains(&failure), "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "A proven spec passes")]
#[test]
fn a_proven_spec_passes() {
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[
            requirement(
                "Matching",
                &["HTTP matching is symmetric", "Defaults apply"],
            ),
            requirement("Wormholes", &["Impact goes through wormholes"]),
        ]),
    )
    .source(
        "tests/matching.rs",
        &proof("queries", "HTTP matching is symmetric"),
    )
    .source("tests/defaults.rs", &proof("queries", "Defaults apply"))
    .source(
        "tests/wormholes.rs",
        &proof("queries", "Impact goes through wormholes"),
    );
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("queries: 3 proven, 0 pending, 0 failing"),
        "{}",
        outcome.output
    );
    for title in [
        "HTTP matching is symmetric",
        "Defaults apply",
        "Impact goes through wormholes",
    ] {
        assert!(
            outcome.output.contains(&format!("proven   {title}")),
            "{}",
            outcome.output
        );
    }
    assert!(
        outcome.output.contains("spec gate passed: 3 scenarios"),
        "{}",
        outcome.output
    );
}

#[gsttt_scenario::scenario("spec-gate", "Ready changes are listed")]
#[test]
fn ready_changes_are_listed() {
    let tree = Tree::new();
    tree.change(
        "finished",
        TICKED,
        &[(
            "queries",
            &delta(
                "ADDED",
                &[requirement("Matching", &["HTTP matching is symmetric"])],
            ),
        )],
    )
    .change(
        "unfinished",
        OPEN,
        &[(
            "queries",
            &delta(
                "ADDED",
                &[requirement("Wormholes", &["Impact goes through wormholes"])],
            ),
        )],
    )
    .source(
        "tests/matching.rs",
        &proof("queries", "HTTP matching is symmetric"),
    )
    .source(
        "tests/wormholes.rs",
        &proof("queries", "Impact goes through wormholes"),
    );
    let outcome = tree.try_run(Mode::Ready).unwrap();
    assert_eq!(outcome.output, "finished\n");
}

#[test]
fn a_finished_change_with_a_scenario_unproven_is_not_ready() {
    let tree = Tree::new();
    tree.change(
        "finished",
        TICKED,
        &[(
            "queries",
            &delta(
                "ADDED",
                &[requirement("Matching", &["HTTP matching is symmetric"])],
            ),
        )],
    );
    let outcome = tree.try_run(Mode::Ready).unwrap();
    assert_eq!(outcome.output, "");
    assert!(outcome.passed, "listing changes does not fail the gate");
}

#[gsttt_scenario::scenario("spec-gate", "A change that names a missing requirement is not ready")]
#[test]
fn a_change_that_names_a_requirement_main_lacks_is_not_ready() {
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[
            requirement("Alpha", &["Alpha works"]),
            requirement("Beta", &["Beta works"]),
            requirement("Gamma", &["Gamma works"]),
            requirement("Delta", &["Delta works"]),
        ]),
    );
    // Every change is finished and proven. Those named `...-existing` find what they name in the
    // main spec; `renames-and-modifies` refers to its modified requirement by the new name.
    let renamed_and_modified = format!(
        "{}\n## MODIFIED Requirements\n\n{}",
        delta("RENAMED", &[renaming("Delta", "Delta renamed")]),
        requirement("Delta renamed", &["Delta works"])
    );
    let changes = [
        (
            "modifies-existing",
            delta("MODIFIED", &[requirement("Alpha", &["Alpha works"])]),
        ),
        (
            "modifies-missing",
            delta("MODIFIED", &[requirement("Nothing", &["Nothing works"])]),
        ),
        ("removes-existing", delta("REMOVED", &[removal("Beta")])),
        ("removes-missing", delta("REMOVED", &[removal("Nothing")])),
        (
            "renames-existing",
            delta("RENAMED", &[renaming("Gamma", "Gamma renamed")]),
        ),
        (
            "renames-missing",
            delta("RENAMED", &[renaming("Nothing", "Something")]),
        ),
        ("renames-and-modifies", renamed_and_modified),
    ];
    for (name, text) in &changes {
        tree.change(name, TICKED, &[("queries", text.as_str())]);
    }
    tree.source("tests/alpha.rs", &proof("queries", "Alpha works"))
        .source("tests/gamma.rs", &proof("queries", "Gamma works"))
        .source("tests/delta.rs", &proof("queries", "Delta works"))
        .source("tests/nothing.rs", &proof("queries", "Nothing works"));
    let outcome = tree.try_run(Mode::Ready).unwrap();
    assert_eq!(
        outcome.output,
        "modifies-existing\nremoves-existing\nrenames-and-modifies\nrenames-existing\n"
    );
}

#[gsttt_scenario::scenario("spec-gate", "A change that a failure names is not ready")]
#[test]
fn a_change_that_a_failure_names_is_not_ready() {
    let tree = Tree::new();
    // The change is finished, adds nothing that lacks a proof and removes a requirement the main
    // spec has, so only the failure of the test that still claims the removed scenario holds it
    // back.
    tree.spec(
        "queries",
        &main_spec(&[requirement("Wormholes", &["Impact goes through wormholes"])]),
    )
    .change(
        "removes-wormholes",
        TICKED,
        &[("queries", &delta("REMOVED", &[removal("Wormholes")]))],
    )
    .source(
        "tests/wormholes.rs",
        &proof("queries", "Impact goes through wormholes"),
    );
    assert_eq!(tree.try_run(Mode::Ready).unwrap().output, "");

    fs::remove_file(tree.dir.path().join("crates/app/tests/wormholes.rs")).unwrap();
    assert_eq!(
        tree.try_run(Mode::Ready).unwrap().output,
        "removes-wormholes\n"
    );
}

#[test]
fn a_change_without_tasks_is_never_ready() {
    let tree = Tree::new();
    tree.change("proposal", "", &[]);
    assert_eq!(tree.try_run(Mode::Ready).unwrap().output, "");
}

#[test]
fn fixtures_and_snapshots_are_left_out() {
    let ghost = proof("queries", "Impact goes through wormholes");
    let tree = Tree::new();
    tree.source("tests/fixtures/ghost.rs", &ghost)
        .source("tests/snapshots/ghost.rs", &ghost)
        .source("fixtures/nested/ghost.rs", &ghost);
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);

    tree.source("tests/ghost.rs", &ghost);
    assert!(!tree.gate().passed, "the same file elsewhere is a proof");
}

#[test]
fn malformed_arguments_are_reported_at_their_line() {
    let tree = Tree::new();
    tree.source(
        "tests/broken.rs",
        "#[scenario(\"only-one\")]\n#[test]\nfn broken() {}\n",
    );
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("crates/app/tests/broken.rs:1: expected two string literals"),
        "{}",
        outcome.output
    );
}

#[test]
fn a_scenario_attribute_off_a_function_is_reported() {
    let tree = Tree::new();
    tree.source(
        "tests/marked.rs",
        "#[scenario(\"queries\", \"Defaults apply\")]\nstruct Marked;\n",
    );
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains(
            "crates/app/tests/marked.rs:1: #[scenario] marks something that is not a function"
        ),
        "{}",
        outcome.output
    );
}

#[test]
fn a_modified_requirement_replaces_the_main_one() {
    let requirement = requirement("Matching", &["HTTP matching is symmetric"]);
    let tree = Tree::new();
    tree.spec("queries", &main_spec(std::slice::from_ref(&requirement)))
        .change(
            "tune-matching",
            OPEN,
            &[("queries", &delta("MODIFIED", &[requirement]))],
        )
        .source(
            "tests/matching.rs",
            &proof("queries", "HTTP matching is symmetric"),
        );
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("queries: 1 proven, 0 pending, 0 failing"),
        "the scenario counts once: {}",
        outcome.output
    );
}

#[test]
fn a_claim_on_a_scenario_that_a_change_retitles_fails() {
    let tree = Tree::new();
    tree.spec(
        "queries",
        &main_spec(&[requirement("Matching", &["Old title"])]),
    )
    .change(
        "retitle",
        OPEN,
        &[(
            "queries",
            &delta("MODIFIED", &[requirement("Matching", &["New title"])]),
        )],
    )
    .source("tests/matching.rs", &proof("queries", "Old title"));
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains(
            "but change `retitle` modifies its requirement `Matching` and no longer has this scenario"
        ),
        "{}",
        outcome.output
    );
}

#[test]
fn a_source_that_does_not_parse_is_an_error_naming_the_file() {
    let tree = Tree::new();
    tree.source("tests/broken.rs", "// scenario\nfn broken( {\n");
    let error = tree.try_run(Mode::Report).unwrap_err();
    assert!(
        error.to_string().contains("crates/app/tests/broken.rs:"),
        "{error}"
    );
}

#[test]
fn unknown_arguments_are_refused() {
    assert!(run(&["--nope".to_owned()]).is_err());
}

#[test]
fn the_flag_picks_the_mode() {
    assert_eq!(mode(&[]).unwrap(), Mode::Report);
    assert_eq!(mode(&["--ready".to_owned()]).unwrap(), Mode::Ready);
    assert!(mode(&["--nope".to_owned()]).is_err());
    assert!(mode(&["--ready".to_owned(), "--ready".to_owned()]).is_err());
}

#[test]
fn the_exit_code_follows_the_verdict() {
    // `ExitCode` cannot be compared, but it can be printed.
    let code = |outcome: &Outcome| format!("{:?}", outcome.exit_code());
    let success = format!("{:?}", ExitCode::SUCCESS);
    let failure = format!("{:?}", ExitCode::FAILURE);

    let passing = Tree::new();
    passing
        .spec(
            "queries",
            &main_spec(&[requirement("Defaults", &["Defaults apply"])]),
        )
        .source("tests/defaults.rs", &proof("queries", "Defaults apply"));
    assert_eq!(code(&passing.gate()), success);

    let failing = Tree::new();
    failing.spec(
        "queries",
        &main_spec(&[requirement("Defaults", &["Defaults apply"])]),
    );
    assert_eq!(code(&failing.gate()), failure);
    // Listing the changes that can be archived never fails.
    assert_eq!(code(&failing.try_run(Mode::Ready).unwrap()), success);
}

#[test]
fn the_workspace_lists_the_targets_cargo_runs_tests_for() {
    let tree = Tree::with_manifest(
        "\n[[test]]\nname = \"outside\"\npath = \"../../shared/outside.rs\"\n\n[[example]]\nname = \"counted\"\ntest = true\n",
    );
    tree.source("src/main.rs", "fn main() {}\n")
        .source("tests/plain.rs", "")
        .source("tests/directory/main.rs", "")
        .source("tests/directory/helper.rs", "")
        .source("examples/demo.rs", "fn main() {}\n")
        .source("examples/counted.rs", "fn main() {}\n")
        .source("benches/speed.rs", "")
        .source("build.rs", "fn main() {}\n")
        .write("shared/outside.rs", "");
    let workspace = tree.workspace();
    // Cargo reports the directory it found by walking up from where it ran, links resolved.
    let root = fs::canonicalize(tree.dir.path()).unwrap();
    assert_eq!(workspace.root, root);
    let [package] = workspace.packages.as_slice() else {
        panic!("one package expected: {workspace:?}");
    };
    assert_eq!(package.dir, root.join("crates/app"));
    let roots: Vec<&Path> = package
        .targets
        .iter()
        .map(|target| target.root.strip_prefix(&root).unwrap())
        .collect();
    let expected: Vec<PathBuf> = [
        "crates/app/examples/counted.rs",
        "crates/app/src/lib.rs",
        "crates/app/src/main.rs",
        "crates/app/tests/directory/main.rs",
        "crates/app/tests/plain.rs",
        "shared/outside.rs",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    assert_eq!(roots, expected);
}

#[test]
fn a_manifest_cargo_cannot_read_is_an_error_naming_the_directory() {
    let tree = Tree::new();
    tree.write("crates/app/Cargo.toml", "this is not a manifest\n");
    let error = Workspace::load(tree.dir.path()).unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("running cargo metadata in ")
            && message.contains(&tree.dir.path().display().to_string()),
        "{message}"
    );
}
