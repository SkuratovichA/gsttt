//! `coverage` and `status`: what the index read, and how it was built.

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{Import, ImportKind, ParseFailure, UnresolvedReason};
use gsttt_core::index::{Freshness, Index, Timings};
use gsttt_core::query::{
    Failure, Reasons, UnclaimedExtension, UnclaimedLanguage, coverage, status,
};
use gsttt_core::testing::{To, Tree, resolving};
use gsttt_scenario::scenario;

/// The reason, the count and the first place of each entry, as a reader of the answer sees them.
fn reasons(list: &[Reasons]) -> Vec<(String, u32, String)> {
    list.iter()
        .map(|entry| {
            (
                entry.reason.to_string(),
                entry.count,
                entry.first.to_string(),
            )
        })
        .collect()
}

fn syntax_error() -> ParseFailure {
    ParseFailure::Syntax {
        line: 3,
        message: "unexpected token".to_owned(),
    }
}

fn too_large() -> ParseFailure {
    ParseFailure::TooLarge {
        bytes: 3_000_000,
        limit: 1_048_576,
    }
}

/// An index with a file that failed to parse, an unresolved import and a `fetch` with a computed URL.
fn troubled() -> Index {
    Tree::new()
        .failed("src/broken.ts", syntax_error())
        .import(
            "src/a.ts",
            Import::new("./gone", ImportKind::Static, 7),
            To::Missing(UnresolvedReason::NotFound),
        )
        .blind(
            "src/client.ts",
            12,
            UnresolvedReason::NonLiteral,
            "fetch(url)",
        )
        .build()
}

#[scenario("queries", "Coverage names failures and blind spots")]
#[test]
fn coverage_names_failures_and_blind_spots() {
    let report = coverage(&troubled(), Freshness::Fresh);

    let [ts] = report.connectors.as_slice() else {
        panic!("expected one connector, got {:?}", report.connectors);
    };
    assert_eq!(ts.connector, "ts");
    assert_eq!(
        ts.failed,
        [Failure {
            path: "src/broken.ts".to_owned(),
            failure: syntax_error()
        }]
    );
    assert_eq!(
        reasons(&ts.unresolved),
        [("no such file".to_owned(), 1, "src/a.ts:7".to_owned())]
    );
    assert_eq!(
        reasons(&ts.unread_sites),
        [(
            "computed at run time".to_owned(),
            1,
            "src/client.ts:12".to_owned()
        )]
    );

    let text = report.to_string();
    for line in [
        "ts failed files (1):\nsrc/broken.ts  syntax error at line 3: unexpected token\n",
        "ts unresolved imports (1):\nno such file  1 import, first at src/a.ts:7\n",
        "ts unread sites (1):\ncomputed at run time  1 site, first at src/client.ts:12\n",
    ] {
        assert!(text.contains(line), "{line:?} is missing from\n{text}");
    }
}

#[test]
fn coverage_counts_files_by_parse_status_and_imports_by_outcome() {
    let text = coverage(&troubled(), Freshness::Fresh).to_string();

    assert!(
        text.starts_with("ts: 3 files in the repository root\n"),
        "{text}"
    );
    assert!(
        text.contains("ts parse status: 2 read cleanly, 0 read past syntax errors, 1 failed\n"),
        "{text}"
    );
    assert!(
        text.contains("ts imports: 1, 0% resolved (0 to files, 0 to packages and builtins)\n"),
        "{text}"
    );
}

#[test]
fn coverage_gives_the_resolved_share_from_the_counts() {
    // 0.29 * 100.0 is 28.999999999999996 as a float, which floors to 28.
    let index = resolving(29, 100).build();

    let text = coverage(&index, Freshness::Fresh).to_string();

    assert!(
        text.contains("ts imports: 100, 29% resolved (0 to files, 29 to packages and builtins)\n"),
        "{text}"
    );
}

#[test]
fn coverage_names_the_projects_and_what_a_connector_provides() {
    let mut index = Tree::new().imports("b.ts", "a.ts").build();
    let ts = &mut index.coverage.connectors[0];
    ts.projects = vec![".".to_owned(), "apps/web".to_owned()];
    ts.provides = vec![Provide {
        kind: ContractKind::Http,
        role: Role::Calls,
        via: vec!["fetch".to_owned(), "axios".to_owned()],
        blind_spots: vec!["blind to URLs built at run time".to_owned()],
    }];

    let text = coverage(&index, Freshness::Fresh).to_string();

    assert!(
        text.starts_with("ts: 2 files in the repository root and apps/web\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "ts provides (1):\nhttp calls via fetch, axios; blind to URLs built at run time\n"
        ),
        "{text}"
    );
}

#[test]
fn coverage_groups_files_no_connector_read_by_language() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .unread("src/a.kt")
        .unread("src/b.kt")
        .unread("tools/run.sh")
        .unread("docs/readme.md")
        .unread("LICENSE")
        .build();

    let report = coverage(&index, Freshness::Fresh);

    assert_eq!(
        report.languages,
        [
            UnclaimedLanguage {
                language: "Kotlin".to_owned(),
                files: 2,
                extensions: vec!["kt".to_owned()]
            },
            UnclaimedLanguage {
                language: "Shell".to_owned(),
                files: 1,
                extensions: vec!["sh".to_owned()]
            }
        ]
    );
    assert_eq!(
        report.other_extensions,
        [
            UnclaimedExtension {
                extension: String::new(),
                files: 1
            },
            UnclaimedExtension {
                extension: "md".to_owned(),
                files: 1
            }
        ]
    );
    let text = report.to_string();
    for line in [
        "files no connector read, by language (2):\nKotlin  2 files (.kt)\nShell  1 file (.sh)\n",
        "other files no connector read, by extension (2):\n(no extension)  1 file\n.md  1 file\n",
    ] {
        assert!(text.contains(line), "{line:?} is missing from\n{text}");
    }
}

#[test]
fn several_extensions_of_one_language_are_one_line() {
    let index = Tree::new()
        .unread("a.js")
        .unread("b.js")
        .unread("c.mjs")
        .build();

    let report = coverage(&index, Freshness::Fresh);

    assert_eq!(
        report.languages,
        [UnclaimedLanguage {
            language: "JavaScript".to_owned(),
            files: 3,
            extensions: vec!["js".to_owned(), "mjs".to_owned()]
        }]
    );
    assert!(
        report
            .to_string()
            .contains("JavaScript  3 files (.js, .mjs)\n")
    );
}

#[test]
fn coverage_lists_the_files_skipped_before_any_connector_read_them() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .failed("bundle.js", too_large())
        .build();

    let report = coverage(&index, Freshness::Fresh);

    assert_eq!(
        report.skipped,
        [Failure {
            path: "bundle.js".to_owned(),
            failure: too_large()
        }]
    );
    assert!(report.to_string().contains(
        "files skipped before any connector read them (1):\nbundle.js  3,000,000 bytes, over the limit of 1,048,576\n"
    ));
}

#[scenario("queries", "A skipped file is owned and not read")]
#[test]
fn a_skipped_file_is_owned_and_not_read() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .failed("bundle.js", too_large())
        .failed("data.ts", ParseFailure::NotUtf8)
        .build();

    let report = coverage(&index, Freshness::Fresh);

    let [ts] = report.connectors.as_slice() else {
        panic!("one connector, got {:?}", report.connectors);
    };
    assert_eq!(
        (
            ts.files,
            ts.parsed,
            ts.recovered,
            ts.failed.len(),
            ts.skipped
        ),
        (4, 2, 0, 0, 2)
    );
    let text = report.to_string();
    assert!(
        text.contains(
            "ts parse status: 2 read cleanly, 0 read past syntax errors, 0 failed, 2 skipped\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("coverage: ts read 2 of 4 files, 100% of imports resolved\n"),
        "{text}"
    );
}

#[test]
fn a_file_is_skipped_for_the_connector_that_owns_it_and_no_other() {
    let index = Tree::new()
        .read_by("python", "tools/p.py")
        .failed("src/bundle.ts", too_large())
        .build();

    let report = coverage(&index, Freshness::Fresh);

    let counts: Vec<(&str, u32, u32, u32)> = report
        .connectors
        .iter()
        .map(|connector| {
            (
                connector.connector.as_str(),
                connector.files,
                connector.parsed,
                connector.skipped,
            )
        })
        .collect();
    assert_eq!(counts, [("python", 1, 1, 0), ("ts", 1, 0, 1)]);
}

#[test]
fn a_parse_status_without_skipped_files_does_not_mention_them() {
    let text = coverage(&troubled(), Freshness::Fresh).to_string();

    assert!(!text.contains("skipped"), "{text}");
}

#[test]
fn coverage_of_an_index_no_connector_read_says_so() {
    let mut index = Tree::new().unread("notes.txt").build();
    index.coverage.connectors.clear();

    let text = coverage(&index, Freshness::Fresh).to_string();

    assert!(text.starts_with("no connector read any file\n"), "{text}");
}

#[scenario("queries", "A stale index says what moved")]
#[test]
fn a_stale_index_says_what_moved() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .commit("a1b2c3d4e5", 0)
        .build();
    let freshness = Freshness::Stale {
        added: 1,
        removed: 0,
        changed: 2,
    };

    let text = status(&index, freshness).to_string();

    assert!(
        text.contains("2 files changed and 1 was added since the index was built\n"),
        "{text}"
    );
}

#[test]
fn status_says_how_the_index_was_built() {
    let mut index = Tree::new()
        .imports("b.ts", "a.ts")
        .commit("a1b2c3d4e5", 3)
        .build();
    index.info.parsed = 120;
    index.info.reused = 4_093;
    index.info.timings = Timings {
        files: 12_300,
        parse: 340_500,
        resolve: 8_000,
        link: 3_200,
        graph: 21_400,
        total: 410_100,
    };

    let answer = status(&index, Freshness::Fresh);

    assert_eq!(answer.built_by, "0.0.0");
    assert_eq!(answer.head.as_deref(), Some("a1b2c3d4e5"));
    assert_eq!((answer.files, answer.dirty), (2, 3));
    assert_eq!((answer.parsed, answer.reused), (120, 4_093));
    let text = answer.to_string();
    for line in [
        "built by gsttt 0.0.0 at a1b2c3d, 3 files dirty\n",
        "2 files: 120 parsed, 4,093 reused\n",
        "took 410.1 ms in all: files 12.3 ms, parse 340.5 ms, resolve 8.0 ms, link 3.2 ms, graph 21.4 ms\n",
        "nothing has changed since the index was built\n",
    ] {
        assert!(text.contains(line), "{line:?} is missing from\n{text}");
    }
}

#[test]
fn status_of_an_index_without_a_commit_names_none() {
    let index = Tree::new().imports("b.ts", "a.ts").build();

    let text = status(&index, Freshness::Fresh).to_string();

    assert!(
        text.starts_with("built by gsttt 0.0.0, 0 files dirty\n"),
        "{text}"
    );
}
