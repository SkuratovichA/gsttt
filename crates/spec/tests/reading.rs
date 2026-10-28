//! Reading OpenSpec files: specs, deltas and task lists, each item with its file and line.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use gsttt_spec::{
    Error, OpenSpec, Problem, Requirement, parse_delta, parse_spec, parse_tasks, read,
};
use proptest::prelude::*;
use tempfile::TempDir;

const SPEC: &str = "\
# Queries Specification

## Purpose
Impact queries.

## Requirements

### Requirement: Impact
The system SHALL answer.

#### Scenario: Direct callers
- **WHEN** a function changes
- **THEN** its callers are listed
- **AND** they are sorted

#### Scenario: Wormholes
- **WHEN** a call crosses a service
- **THEN** it is listed

### Requirement: Limits
The system SHALL cap.

#### Scenario: Depth cap
- **WHEN** the depth is 3
- **THEN** the walk stops
";

const DELTA: &str = "\
# Spec Delta

## Purpose
Changes.

## ADDED Requirements

### Requirement: New one
The system SHALL add.

#### Scenario: Fresh
- **WHEN** a
- **THEN** b

## MODIFIED Requirements

### Requirement: Changed one
The system SHALL change.

#### Scenario: Rewritten
- **WHEN** c
- **THEN** d

## REMOVED Requirements

### Requirement: Gone one
**Reason**: obsolete

- `### Requirement: Also gone`

## RENAMED Requirements

- FROM: `### Requirement: Old name`
- TO: `### Requirement: New name`
";

/// Writes `files` (path, text) under a fresh temporary directory.
#[expect(
    clippy::unwrap_used,
    reason = "a test helper: a tree that cannot be written should stop the test"
)]
fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, text) in files {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    dir
}

/// The title and line of every scenario, in file order.
fn scenarios(requirements: &[Requirement]) -> Vec<(&str, usize)> {
    requirements
        .iter()
        .flat_map(|r| r.scenarios.iter().map(|s| (s.title.as_str(), s.line)))
        .collect()
}

/// The scenarios the reader found in each file, as (line, title) sorted by line.
fn found_in_files(open: &OpenSpec) -> Vec<(PathBuf, Vec<(usize, String)>)> {
    let mut files = Vec::new();
    let collect = |requirements: Vec<&Requirement>| {
        let mut found: Vec<(usize, String)> = requirements
            .iter()
            .flat_map(|r| &r.scenarios)
            .map(|s| (s.line, s.title.clone()))
            .collect();
        found.sort();
        found
    };
    for spec in &open.specs {
        files.push((
            spec.file.clone(),
            collect(spec.requirements.iter().collect()),
        ));
    }
    for delta in open.changes.iter().flat_map(|change| &change.deltas) {
        files.push((
            delta.file.clone(),
            collect(delta.added.iter().chain(&delta.modified).collect()),
        ));
    }
    files
}

#[gsttt_scenario::scenario("spec-gate", "The repository's own specs read cleanly")]
#[test]
fn repository_specs_read_cleanly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let open = read(&root).unwrap();
    let mut total = 0;
    for (file, found) in found_in_files(&open) {
        // A plain scan of the file, so that a mistake in the reader cannot hide behind itself. It
        // assumes no spec shows a scenario heading inside a code fence or under a removed requirement.
        let source = fs::read_to_string(root.join(&file)).unwrap();
        let expected: Vec<(usize, String)> = source
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                line.strip_prefix("#### Scenario:")
                    .map(|title| (index + 1, title.trim().to_owned()))
            })
            .collect();
        assert_eq!(found, expected, "{}", file.display());
        total += expected.len();
    }
    assert!(
        total > 0,
        "the repository has scenarios, so the reader must find some"
    );
}

#[gsttt_scenario::scenario("spec-gate", "A malformed scenario is named")]
#[test]
fn malformed_scenario_is_named() {
    let root = tree(&[(
        "openspec/specs/queries/spec.md",
        "# Queries Specification\n\n## Requirements\n\n### Requirement: Impact\nThe system SHALL answer.\n\n#### Scenario: No trigger\n- **THEN** something happens\n",
    )]);
    let error = read(root.path()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "openspec/specs/queries/spec.md:8: scenario `No trigger` has no WHEN line; expected `- **WHEN** …` under its heading"
    );
    assert!(matches!(
        error,
        Error::Format {
            line: 8,
            problem: Problem::ScenarioStep {
                keyword: "WHEN",
                ..
            },
            ..
        }
    ));
}

#[gsttt_scenario::scenario("spec-gate", "An unclosed code fence is named")]
#[test]
fn an_unclosed_code_fence_is_named() {
    let hidden = "#### Scenario: Hidden\n- **WHEN** c\n- **THEN** d\n";
    let spec = |fence: &str| {
        format!(
            "## Requirements\n\n### Requirement: R\nSHALL.\n\n#### Scenario: S\n- **WHEN** a\n- **THEN** b\n\n{fence}\n{hidden}"
        )
    };
    let delta = format!(
        "## ADDED Requirements\n\n### Requirement: R\nSHALL.\n\n#### Scenario: S\n- **WHEN** a\n- **THEN** b\n\n```\n{hidden}"
    );
    // The fence opens on line 10 of each, and the only thing after it is the hidden scenario.
    let cases = [
        ("backticks", spec("```text"), 10),
        ("tildes", spec("~~~"), 10),
        // A fence of four is not closed by a line of three.
        ("a shorter closing line", spec("````\n```"), 10),
        ("a delta", delta, 10),
        // The fence hides the `THEN` step of the scenario above it: the fence is the problem, not
        // the missing step.
        (
            "a fence over a step",
            "## Requirements\n\n### Requirement: R\nSHALL.\n\n#### Scenario: S\n- **WHEN** a\n```text\n- **THEN** b\n".to_owned(),
            8,
        ),
    ];
    for (name, text, line) in cases {
        let file = Path::new("openspec/specs/q/spec.md");
        let error = if name == "a delta" {
            parse_delta("q", file, &text).unwrap_err()
        } else {
            parse_spec("q", file, &text).unwrap_err()
        };
        assert!(
            matches!(
                &error,
                Error::Format { line: found, problem: Problem::UnclosedFence, .. } if *found == line
            ),
            "{name}: {error}"
        );
    }

    let root = tree(&[("openspec/specs/queries/spec.md", &spec("```"))]);
    assert_eq!(
        read(root.path()).unwrap_err().to_string(),
        "openspec/specs/queries/spec.md:10: code fence is never closed, so the rest of the file counts as code; expected a closing fence"
    );
}

#[test]
fn an_unclosed_fence_in_a_task_list_hides_no_task() {
    // `openspec list` counts the checkboxes in code too, so the task list has nothing to hide.
    let tasks = parse_tasks("- [x] first\n```\n- [ ] second\n");
    assert_eq!(
        tasks.iter().map(|task| task.done).collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn a_malformed_delta_is_named_with_its_own_file() {
    let root = tree(&[
        ("openspec/specs/queries/spec.md", SPEC),
        (
            "openspec/changes/add-things/specs/queries/spec.md",
            "## ADDED Requirements\n\n### Requirement: R\nSHALL.\n\n#### Scenario: Half\n- **WHEN** a\n",
        ),
    ]);
    let error = read(root.path()).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("openspec/changes/add-things/specs/queries/spec.md:6: "),
        "{error}"
    );
}

#[test]
fn spec_scenarios_come_with_their_lines() {
    let spec = parse_spec("queries", Path::new("openspec/specs/queries/spec.md"), SPEC).unwrap();
    assert_eq!(spec.capability, "queries");
    let requirements: Vec<_> = spec
        .requirements
        .iter()
        .map(|r| (r.name.as_str(), r.line))
        .collect();
    assert_eq!(requirements, [("Impact", 8), ("Limits", 20)]);
    assert_eq!(
        scenarios(&spec.requirements),
        [("Direct callers", 11), ("Wormholes", 16), ("Depth cap", 23)]
    );
}

#[test]
fn every_delta_section_is_read() {
    let delta = parse_delta(
        "queries",
        Path::new("openspec/changes/c/specs/queries/spec.md"),
        DELTA,
    )
    .unwrap();
    assert_eq!(
        delta
            .added
            .iter()
            .map(|r| (r.name.as_str(), r.line))
            .collect::<Vec<_>>(),
        [("New one", 8)]
    );
    assert_eq!(scenarios(&delta.added), [("Fresh", 11)]);
    assert_eq!(
        delta
            .modified
            .iter()
            .map(|r| (r.name.as_str(), r.line))
            .collect::<Vec<_>>(),
        [("Changed one", 17)]
    );
    assert_eq!(scenarios(&delta.modified), [("Rewritten", 20)]);
    let removed: Vec<_> = delta
        .removed
        .iter()
        .map(|r| (r.name.as_str(), r.line))
        .collect();
    assert_eq!(removed, [("Gone one", 26), ("Also gone", 29)]);
    let renamed: Vec<_> = delta
        .renamed
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str(), r.line))
        .collect();
    assert_eq!(renamed, [("Old name", "New name", 33)]);
}

#[test]
fn fenced_code_is_not_read() {
    let text = "## Requirements\n\n### Requirement: Real\nSHALL.\n\n```markdown\n### Requirement: Fake\n#### Scenario: Fake\n- **WHEN** x\n- **THEN** y\n```\n\n#### Scenario: Real one\n- **WHEN** a\n- **THEN** b\n";
    let spec = parse_spec("q", Path::new("q.md"), text).unwrap();
    assert_eq!(
        spec.requirements
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["Real"]
    );
    assert_eq!(scenarios(&spec.requirements), [("Real one", 13)]);
}

#[test]
fn steps_inside_a_fence_do_not_count() {
    let text = "## Requirements\n\n### Requirement: R\nSHALL.\n\n#### Scenario: Only in a fence\n```\n- **WHEN** x\n- **THEN** y\n```\n";
    let error = parse_spec("q", Path::new("q.md"), text).unwrap_err();
    assert!(matches!(
        error,
        Error::Format {
            line: 6,
            problem: Problem::ScenarioStep {
                keyword: "WHEN",
                ..
            },
            ..
        }
    ));
}

#[test]
fn spec_errors_name_the_line_and_what_was_expected() {
    let cases = [
        (
            "## Requirements\n\n### Requirement: R\nSHALL.\n\n#### Scenario: S\n- **WHEN** a\n",
            6,
            "scenario `S` has no THEN line",
        ),
        (
            "## Requirements\n\n### Requirement: R\n\n#### Given something\n",
            5,
            "expected `#### Scenario: <title>`, found `Given something`",
        ),
        (
            "## Requirements\n\n#### Scenario: S\n- **WHEN** a\n- **THEN** b\n",
            3,
            "scenario `S` is not under a requirement",
        ),
        (
            "## Requirements\n\n### Rule: R\n",
            3,
            "expected `### Requirement: <name>`, found `Rule: R`",
        ),
        (
            "## Purpose\n\n### Requirement: R\n",
            3,
            "requirement `R` is outside the sections that hold requirements",
        ),
    ];
    for (text, line, message) in cases {
        let shown = parse_spec("q", Path::new("openspec/specs/q/spec.md"), text)
            .unwrap_err()
            .to_string();
        assert!(
            shown.starts_with(&format!("openspec/specs/q/spec.md:{line}: "))
                && shown.contains(message),
            "{shown}"
        );
    }
}

#[test]
fn delta_errors_name_the_line_and_what_was_expected() {
    let cases = [
        (
            "# Spec Delta\n\n## Purpose\nNothing.\n",
            1,
            "expected a `## ADDED Requirements`",
        ),
        (
            "## RENAMED Requirements\n\n- FROM: `### Requirement: Old`\n",
            3,
            "`FROM: ### Requirement: Old` has no TO",
        ),
        (
            "## RENAMED Requirements\n\n- TO: `### Requirement: New`\n",
            3,
            "`TO: ### Requirement: New` has no FROM",
        ),
        (
            "## RENAMED Requirements\n\n- FROM: `### Requirement: A`\n- FROM: `### Requirement: B`\n- TO: `### Requirement: C`\n",
            3,
            "`FROM: ### Requirement: A` has no TO",
        ),
        (
            "## REMOVED Requirements\n\n### Gone one\n",
            3,
            "expected `### Requirement: <name>`, found `Gone one`",
        ),
    ];
    for (text, line, message) in cases {
        let shown = parse_delta("q", Path::new("openspec/changes/c/specs/q/spec.md"), text)
            .unwrap_err()
            .to_string();
        assert!(
            shown.starts_with(&format!("openspec/changes/c/specs/q/spec.md:{line}: "))
                && shown.contains(message),
            "{shown}"
        );
    }
}

#[test]
fn tasks_are_counted_the_way_openspec_counts_them() {
    let text = "# Tasks\n\n## 1. Reader\n- [x] 1.1 Done\n- [ ] 1.2 Open\n  - [X] nested upper\n* [~] 1.3 half\n1. [x] numbered\n2) [ ] paren\n- [x](link) not a task\n- [] empty box\n- plain bullet\nnot a list [x]\n";
    let found: Vec<_> = parse_tasks(text)
        .into_iter()
        .map(|task| (task.line, task.done, task.text))
        .collect();
    let expected = [
        (4, true, "1.1 Done"),
        (5, false, "1.2 Open"),
        (6, true, "nested upper"),
        (7, false, "1.3 half"),
        (8, true, "numbered"),
        (9, false, "paren"),
        (11, false, "empty box"),
    ]
    .map(|(line, done, text)| (line, done, text.to_owned()));
    assert_eq!(found, expected);
}

#[test]
fn a_change_is_finished_when_it_has_tasks_and_all_are_ticked() {
    let root = tree(&[
        ("openspec/specs/queries/spec.md", SPEC),
        ("openspec/specs/nested/deeper/spec.md", SPEC),
        ("openspec/changes/done/tasks.md", "- [x] a\n- [x] b\n"),
        ("openspec/changes/done/specs/queries/spec.md", DELTA),
        ("openspec/changes/open/tasks.md", "- [x] a\n- [ ] b\n"),
        ("openspec/changes/proposal/proposal.md", "# Proposal\n"),
        (
            "openspec/changes/archive/2026-01-01-old/tasks.md",
            "- [ ] stale\n",
        ),
    ]);
    let open = read(root.path()).unwrap();
    let capabilities: Vec<_> = open
        .specs
        .iter()
        .map(|spec| spec.capability.as_str())
        .collect();
    assert_eq!(capabilities, ["nested/deeper", "queries"]);
    let changes: Vec<_> = open
        .changes
        .iter()
        .map(|change| (change.name.as_str(), change.is_finished()))
        .collect();
    assert_eq!(
        changes,
        [("done", true), ("open", false), ("proposal", false)]
    );
    let delta = &open.changes[0].deltas[0];
    assert_eq!(delta.capability, "queries");
    assert_eq!(
        delta.file,
        Path::new("openspec/changes/done/specs/queries/spec.md")
    );
    assert_eq!(
        open.changes[0].tasks_file,
        Path::new("openspec/changes/done/tasks.md")
    );
}

#[test]
fn a_tree_without_openspec_is_an_error() {
    let root = tree(&[]);
    let error = read(root.path()).unwrap_err();
    assert!(
        matches!(&error, Error::Read { path, .. } if path == Path::new("openspec")),
        "{error}"
    );
}

/// Not annotated: a test marked `#[cfg(unix)]` cannot carry `#[scenario]`, because the gate takes any
/// condition but `test` for a proof that may not run.
#[cfg(unix)]
mod unix {
    use std::os::unix::fs::symlink;

    use super::*;

    fn capabilities(open: &OpenSpec) -> Vec<&str> {
        open.specs
            .iter()
            .map(|spec| spec.capability.as_str())
            .collect()
    }

    #[test]
    fn a_symbolic_link_back_into_the_tree_is_not_followed() {
        let root = tree(&[("openspec/specs/queries/spec.md", SPEC)]);
        // `openspec/specs/queries/back` leads to `openspec/specs`, which holds `queries` again.
        symlink("..", root.path().join("openspec/specs/queries/back")).unwrap();
        let open = read(root.path()).unwrap();
        assert_eq!(capabilities(&open), ["queries"]);
    }

    #[test]
    fn a_symbolic_link_to_a_directory_is_no_change() {
        let root = tree(&[
            ("openspec/specs/queries/spec.md", SPEC),
            ("elsewhere/add-things/tasks.md", "- [ ] a\n"),
        ]);
        fs::create_dir_all(root.path().join("openspec/changes")).unwrap();
        symlink(
            root.path().join("elsewhere/add-things"),
            root.path().join("openspec/changes/add-things"),
        )
        .unwrap();
        let open = read(root.path()).unwrap();
        assert!(open.changes.is_empty(), "{:?}", open.changes);
    }
}

proptest! {
    #[test]
    fn task_boxes_round_trip(boxes in prop::collection::vec((any::<bool>(), 0usize..4), 0..20)) {
        let mut text = String::new();
        for (index, (done, indent)) in boxes.iter().enumerate() {
            let mark = if *done { 'x' } else { ' ' };
            writeln!(text, "{}- [{mark}] task {index}", " ".repeat(*indent)).unwrap();
        }
        let tasks = parse_tasks(&text);
        prop_assert_eq!(tasks.len(), boxes.len());
        for (index, (task, (done, _))) in tasks.iter().zip(&boxes).enumerate() {
            prop_assert_eq!(task.done, *done);
            prop_assert_eq!(task.line, index + 1);
            prop_assert_eq!(&task.text, &format!("task {index}"));
        }
    }

    #[test]
    fn scenario_titles_round_trip(titles in prop::collection::vec("[A-Za-z][A-Za-z0-9 ,'`.-]{0,30}[A-Za-z0-9]", 1..6)) {
        let mut text = String::from("## Requirements\n\n### Requirement: R\nSHALL.\n");
        for title in &titles {
            write!(text, "\n#### Scenario: {title}\n- **WHEN** a\n- **THEN** b\n").unwrap();
        }
        let spec = parse_spec("q", Path::new("q.md"), &text).unwrap();
        let found: Vec<_> = spec.requirements[0].scenarios.iter().map(|s| (s.title.clone(), s.line)).collect();
        let expected: Vec<_> = titles.iter().enumerate().map(|(index, title)| (title.clone(), 6 + 4 * index)).collect();
        prop_assert_eq!(found, expected);
    }
}

#[test]
fn a_purpose_is_the_prose_of_its_section_and_the_line_it_starts_on() {
    let spec = parse_spec("queries", Path::new("spec.md"), SPEC).unwrap();
    let purpose = spec.purpose.unwrap();
    assert_eq!(
        (purpose.text.as_str(), purpose.line),
        ("Impact queries.", 4)
    );
    let delta = parse_delta("queries", Path::new("delta.md"), DELTA).unwrap();
    let purpose = delta.purpose.unwrap();
    assert_eq!((purpose.text.as_str(), purpose.line), ("Changes.", 4));

    // Blank lines and fenced code are no prose; the lines that remain are trimmed and joined.
    let text = "# Spec\n\n## Purpose\n\n  First line.\n```\nnot prose\n```\n\nSecond line.\n\n## Requirements\n";
    let purpose = parse_spec("queries", Path::new("spec.md"), text)
        .unwrap()
        .purpose
        .unwrap();
    assert_eq!(
        (purpose.text.as_str(), purpose.line),
        ("First line.\nSecond line.", 5)
    );

    // No section, or one with no prose, is no purpose.
    for text in [
        "# Spec\n\n## Requirements\n",
        "# Spec\n\n## Purpose\n\n## Requirements\n",
    ] {
        let spec = parse_spec("queries", Path::new("spec.md"), text).unwrap();
        assert_eq!(spec.purpose, None);
    }
}

#[test]
fn a_delta_with_a_purpose_and_no_section_is_still_an_error() {
    let text = "# Spec Delta\n\n## Purpose\nChanges.\n";
    let error = parse_delta("queries", Path::new("delta.md"), text).unwrap_err();
    assert!(matches!(
        error,
        Error::Format {
            line: 1,
            problem: Problem::NoDeltaSection,
            ..
        }
    ));
}
