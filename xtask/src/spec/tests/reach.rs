//! Which sources the gate reads for proofs: the ones a library, binary or test target compiles, and
//! what keeps a test in them from running.

use std::fs;

use super::{Mode, Tree, line_of, main_spec, proof, requirement};

/// The only scenario of the specs in these tests.
const TITLE: &str = "Defaults apply";

/// A tree whose main spec has one scenario, [`TITLE`], in the capability `queries`.
fn tree_with_scenario(manifest: &str) -> Tree {
    let tree = Tree::with_manifest(manifest);
    tree.spec("queries", &main_spec(&[requirement("Defaults", &[TITLE])]));
    tree
}

/// A test that proves [`TITLE`], with `before` between its attributes and its `fn` line.
fn claim(before: &str) -> String {
    format!("#[scenario(\"queries\", \"{TITLE}\")]\n#[test]\n{before}fn proof() {{}}\n")
}

/// The line the gate prints for the `#[scenario]` on the `proof` function of `source`, which is in
/// `file` and proves nothing for the reason `why`.
fn misuse(file: &str, source: &str, why: &str) -> String {
    format!(
        "{file}:{}: `proof` is marked with #[scenario] but {why}, so it proves nothing",
        line_of(source, "fn proof")
    )
}

const UNREACHED: &str =
    "its file is part of no library, binary or test target: no `mod` declaration reaches it";

#[gsttt_scenario::scenario("spec-gate", "Modules of a target are followed")]
#[test]
fn modules_of_a_target_are_followed() {
    // Each file proves a scenario titled with its own path, and no target names it but through the
    // `mod` declarations of the files above it.
    let files = [
        (
            "src/lib.rs",
            "#[cfg(test)]\nmod tests;\nmod outer;\nmod deep;\nmod wrapper {\n    mod inside;\n}\n#[path = \"other/place.rs\"]\nmod place;\n",
        ),
        ("src/tests.rs", ""),
        // A file that is not a `mod.rs` looks for its modules in the directory of its name.
        (
            "src/outer.rs",
            "mod inner;\n#[path = \"elsewhere.rs\"]\nmod elsewhere;\nmod n {\n    #[path = \"q.rs\"]\n    mod q;\n}\n",
        ),
        ("src/outer/inner.rs", ""),
        // A `#[path]` outside an inline module starts at the directory of the declaring file...
        ("src/elsewhere.rs", ""),
        // ... and inside one at the directory of the module, `outer/n` here.
        ("src/outer/n/q.rs", ""),
        ("src/deep/mod.rs", "mod deeper;\n"),
        ("src/deep/deeper.rs", ""),
        ("src/wrapper/inside.rs", ""),
        // A file named by `#[path]` looks for its modules beside itself.
        ("src/other/place.rs", "mod near;\n"),
        ("src/other/near.rs", ""),
        ("tests/suite.rs", "mod common;\n"),
        ("tests/common/mod.rs", ""),
    ];
    let titles: Vec<&str> = files.iter().map(|(file, _)| *file).collect();
    let tree = Tree::new();
    tree.spec("queries", &main_spec(&[requirement("Layout", &titles)]));
    for (file, declarations) in files {
        tree.source(file, &format!("{declarations}{}", proof("queries", file)));
    }
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    let proven = format!("queries: {} proven, 0 pending, 0 failing", files.len());
    assert!(outcome.output.contains(&proven), "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "A target outside the member is read")]
#[test]
fn a_target_outside_the_member_is_read() {
    let tree =
        tree_with_scenario("\n[[test]]\nname = \"outside\"\npath = \"../../shared/outside.rs\"\n");
    tree.write("shared/outside.rs", &claim(""));
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains("shared/outside.rs:3 `proof`"),
        "{}",
        outcome.output
    );
}

#[gsttt_scenario::scenario("spec-gate", "A file no target reaches is no proof")]
#[test]
fn a_file_no_target_reaches_is_no_proof() {
    let source = claim("");
    let tree = tree_with_scenario("");
    tree.source("tests/orphan/helpers.rs", &source);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    let named = misuse("crates/app/tests/orphan/helpers.rs", &source, UNREACHED);
    assert!(outcome.output.contains(&named), "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("scenario `Defaults apply` of capability `queries` has no proof"),
        "{}",
        outcome.output
    );

    // The same file is a proof as soon as a target declares it.
    tree.source("tests/suite.rs", "mod orphan {\n    mod helpers;\n}\n");
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
}

#[test]
fn examples_and_benches_are_no_test_targets() {
    let source = claim("");
    let tree = tree_with_scenario("");
    tree.source("examples/demo.rs", &source)
        .source("benches/speed.rs", &source);
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    for file in ["examples/demo.rs", "benches/speed.rs"] {
        let named = misuse(&format!("crates/app/{file}"), &source, UNREACHED);
        assert!(outcome.output.contains(&named), "{}", outcome.output);
    }
}

#[test]
fn an_example_whose_tests_the_manifest_turns_on_is_a_test_target() {
    let tree = tree_with_scenario("\n[[example]]\nname = \"demo\"\ntest = true\n");
    tree.source("examples/demo.rs", &claim(""));
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
}

#[gsttt_scenario::scenario("spec-gate", "An ignored test is no proof")]
#[test]
fn an_ignored_test_is_no_proof() {
    let cases = [
        ("#[ignore]\n", "is #[ignore]d"),
        ("#[ignore = \"slow\"]\n", "is #[ignore]d"),
        (
            "#[cfg_attr(feature = \"slow\", ignore)]\n",
            "is ignored under cfg_attr(feature = \"slow\", ignore)",
        ),
    ];
    for (attribute, why) in cases {
        let source = claim(attribute);
        let tree = tree_with_scenario("");
        tree.source("tests/defaults.rs", &source);
        let outcome = tree.gate();
        assert!(!outcome.passed, "{attribute}: {}", outcome.output);
        let named = misuse("crates/app/tests/defaults.rs", &source, why);
        assert!(
            outcome.output.contains(&named),
            "{attribute}: {}",
            outcome.output
        );
    }
}

/// `body` inside the module declared by `header`, indented one level.
fn inside(header: &str, body: &str) -> String {
    let indented: Vec<String> = body.lines().map(|line| format!("    {line}\n")).collect();
    format!("{header} {{\n{}}}\n", indented.concat())
}

/// A tree in which a test that claims the scenario proves nothing, and what is to be said of it.
struct Case {
    /// Added to the manifest of the member.
    manifest: &'static str,
    /// The files of the member.
    files: Vec<(&'static str, String)>,
    /// The file whose test proves nothing.
    proving: &'static str,
    /// The reason it proves nothing.
    why: &'static str,
}

#[gsttt_scenario::scenario("spec-gate", "A conditional test is no proof")]
#[test]
fn a_conditional_test_is_no_proof() {
    let slow = "is compiled only under cfg(feature = \"slow\")";
    let cases = [
        Case {
            manifest: "",
            files: vec![("tests/defaults.rs", claim("#[cfg(feature = \"slow\")]\n"))],
            proving: "tests/defaults.rs",
            why: slow,
        },
        Case {
            manifest: "",
            files: vec![(
                "tests/defaults.rs",
                inside("#[cfg(feature = \"slow\")]\nmod slow", &claim("")),
            )],
            proving: "tests/defaults.rs",
            why: slow,
        },
        Case {
            manifest: "",
            files: vec![(
                "tests/defaults.rs",
                format!("#![cfg(feature = \"slow\")]\n{}", claim("")),
            )],
            proving: "tests/defaults.rs",
            why: slow,
        },
        Case {
            manifest: "",
            files: vec![
                (
                    "tests/suite.rs",
                    "#[cfg(feature = \"slow\")]\nmod common;\n".to_owned(),
                ),
                ("tests/common/mod.rs", claim("")),
            ],
            proving: "tests/common/mod.rs",
            why: slow,
        },
        Case {
            manifest: "\n[[test]]\nname = \"defaults\"\nrequired-features = [\"slow\"]\n",
            files: vec![("tests/defaults.rs", claim(""))],
            proving: "tests/defaults.rs",
            why: "its target `defaults` is built only with the cargo features `slow`",
        },
    ];
    for case in cases {
        let tree = tree_with_scenario(case.manifest);
        for (file, text) in &case.files {
            tree.source(file, text);
        }
        let source = case
            .files
            .iter()
            .find(|(file, _)| *file == case.proving)
            .map(|(_, text)| text.as_str())
            .unwrap();
        let outcome = tree.gate();
        assert!(!outcome.passed, "{}: {}", case.proving, outcome.output);
        let named = misuse(&format!("crates/app/{}", case.proving), source, case.why);
        assert!(
            outcome.output.contains(&named),
            "{}: {}",
            case.proving,
            outcome.output
        );
    }
}

#[test]
fn a_test_module_under_cfg_test_alone_is_a_proof() {
    let tree = tree_with_scenario("");
    tree.source(
        "tests/defaults.rs",
        &inside("#[cfg(test)]\nmod tests", &claim("")),
    );
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
}

#[test]
fn a_file_one_target_declares_plainly_is_compiled_whatever_another_declares() {
    // `a` comes first and declares the module under a cfg; `b` declares it for sure.
    let tree = tree_with_scenario("");
    tree.source("tests/a.rs", "#[cfg(feature = \"slow\")]\nmod common;\n")
        .source("tests/b.rs", "mod common;\n")
        .source("tests/common/mod.rs", &claim(""));
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
}

#[test]
fn a_file_that_two_targets_declare_counts_once() {
    let tree = tree_with_scenario("");
    tree.source("tests/a.rs", "mod common;\n")
        .source("tests/b.rs", "mod common;\n")
        .source("tests/common/mod.rs", &claim(""));
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("queries: 1 proven, 0 pending, 0 failing"),
        "{}",
        outcome.output
    );
}

#[test]
fn a_module_without_a_file_is_an_error_naming_its_declaration() {
    let tree = tree_with_scenario("");
    tree.source("tests/suite.rs", "mod missing;\n");
    let error = tree.try_run(Mode::Report).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "crates/app/tests/suite.rs:1: cannot find the file of module `missing`; looked for crates/app/tests/missing.rs and crates/app/tests/missing/mod.rs"
    );

    // A cfg may keep the compiler from looking for the file at all.
    tree.source(
        "tests/suite.rs",
        "#[cfg(feature = \"slow\")]\nmod missing;\n",
    );
    tree.try_run(Mode::Report).unwrap();
}

#[gsttt_scenario::scenario("spec-gate", "Copies of the tree are left out")]
#[test]
fn copies_of_the_tree_inside_a_package_are_left_out() {
    let ghost = proof("queries", "Impact goes through wormholes");
    // A package at the root holds the whole tree, copies of it included. Each copy is kept out by
    // one rule alone: its directory is `target`, it is hidden, or it has a manifest of its own.
    let tree = Tree::rooted();
    tree.spec("queries", &main_spec(&[requirement("Defaults", &[TITLE])]))
        .write("tests/defaults.rs", &claim(""))
        .write("target/debug/copy/tests/ghost.rs", &ghost)
        .write(".claude/worktrees/other/tests/ghost.rs", &ghost)
        .write("vendor/inner/tests/ghost.rs", &ghost)
        .write("vendor/inner/Cargo.toml", "[package]\nname = \"inner\"\n");
    let outcome = tree.gate();
    assert!(outcome.passed, "{}", outcome.output);
    assert!(
        outcome
            .output
            .contains("queries: 1 proven, 0 pending, 0 failing"),
        "{}",
        outcome.output
    );
}

#[test]
fn a_proptest_attribute_that_does_not_parse_is_reported() {
    let tree = tree_with_scenario("");
    tree.source(
        "tests/defaults.rs",
        "proptest! {\n    #[1]\n    #[scenario(\"queries\", \"Defaults apply\")]\n    fn proof() {}\n}\n",
    );
    let outcome = tree.gate();
    assert!(!outcome.passed, "{}", outcome.output);
    assert!(
        outcome.output.contains(
            "crates/app/tests/defaults.rs:2: cannot read this attribute inside proptest!, so the gate cannot tell what it marks:"
        ),
        "{}",
        outcome.output
    );
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::path::{Path, PathBuf};

    use super::*;

    /// A file or directory with its permissions taken away until the value is dropped.
    struct Locked {
        path: PathBuf,
        mode: u32,
    }

    impl Locked {
        /// `None` when the lock changes nothing, which is so for root.
        fn new(path: &Path) -> Option<Self> {
            let mode = fs::metadata(path).unwrap().permissions().mode();
            fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
            let locked = Self {
                path: path.to_path_buf(),
                mode,
            };
            fs::File::open(path).is_err().then_some(locked)
        }
    }

    impl Drop for Locked {
        fn drop(&mut self) {
            // Without the permissions back, the temporary directory cannot be removed.
            fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode)).ok();
        }
    }

    #[test]
    fn an_unreadable_directory_is_named_in_the_error() {
        let tree = tree_with_scenario("");
        tree.source("data/locked/notes.rs", "");
        let Some(_lock) = Locked::new(&tree.dir.path().join("crates/app/data/locked")) else {
            return;
        };
        let error = tree.try_run(Mode::Report).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("listing ") && message.contains("crates/app/data/locked"),
            "{message}"
        );
    }

    #[test]
    fn an_unreadable_source_is_named_in_the_error() {
        let tree = tree_with_scenario("");
        tree.source("tests/secret.rs", "");
        let Some(_lock) = Locked::new(&tree.dir.path().join("crates/app/tests/secret.rs")) else {
            return;
        };
        let error = tree.try_run(Mode::Report).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("reading ") && message.contains("crates/app/tests/secret.rs"),
            "{message}"
        );
    }

    #[test]
    fn a_symbolic_link_is_not_followed() {
        // Followed, the link would show the proof again at every depth the system allows, as a
        // file that no target reaches.
        let tree = tree_with_scenario("");
        tree.source("tests/plain.rs", &claim(""));
        symlink(
            tree.dir.path().join("crates/app/tests"),
            tree.dir.path().join("crates/app/tests/loop"),
        )
        .unwrap();
        let outcome = tree.gate();
        assert!(outcome.passed, "{}", outcome.output);
    }
}
