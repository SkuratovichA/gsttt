//! What every answer shares: cut lists, what was searched, and the footer.

use gsttt_core::fact::{Import, ImportKind, ParseFailure, UnresolvedReason};
use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{
    Answer, Entries, ImpactRequest, contract, coverage, dependencies, dependents, impact, path,
    status,
};
use gsttt_core::testing::{To, Tree, package, resolving};
use gsttt_scenario::scenario;

/// A tree where `count` files import `src/hub.ts`.
fn hub(count: usize) -> Index {
    (0..count)
        .fold(Tree::new(), |tree, n| {
            tree.imports(&format!("src/user{n:02}.ts"), "src/hub.ts")
        })
        .build()
}

fn lines_starting_with<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines()
        .filter(|line| line.starts_with(prefix))
        .collect()
}

#[scenario("queries", "Long lists are cut with a count")]
#[test]
fn long_lists_are_cut_with_a_count() {
    let answer = dependents(&hub(45), Freshness::Fresh, "src/hub.ts");

    let text = answer.to_string();

    assert_eq!(lines_starting_with(&text, "src/user").len(), 30);
    assert_eq!(lines_starting_with(&text, "… "), ["… 15 more"]);
    assert!(text.contains("dependents of src/hub.ts (45):\n"), "{text}");

    let every = answer.text(Entries::All);
    assert_eq!(lines_starting_with(&every, "src/user").len(), 45);
    assert_eq!(lines_starting_with(&every, "… "), Vec::<&str>::new());
}

#[test]
fn a_list_of_exactly_the_cap_is_not_cut() {
    let text = dependents(&hub(30), Freshness::Fresh, "src/hub.ts").to_string();

    assert_eq!(lines_starting_with(&text, "src/user").len(), 30);
    assert_eq!(lines_starting_with(&text, "… "), Vec::<&str>::new());
}

#[test]
fn the_cut_keeps_the_first_entries_in_order() {
    let text = dependents(&hub(45), Freshness::Fresh, "src/hub.ts").to_string();

    let shown = lines_starting_with(&text, "src/user");
    assert!(
        shown
            .first()
            .is_some_and(|line| line.starts_with("src/user00.ts"))
    );
    assert!(
        shown
            .last()
            .is_some_and(|line| line.starts_with("src/user29.ts"))
    );
}

#[test]
fn every_list_of_an_answer_is_cut_on_its_own() {
    let tree = (0..40).fold(Tree::new(), |tree, n| {
        tree.imports("src/a.ts", &format!("src/dep{n:02}.ts"))
            .import(
                "src/a.ts",
                Import::new(format!("pkg{n:02}"), ImportKind::Static, 100 + n),
                package(&format!("pkg{n:02}")),
            )
    });

    let text = dependencies(&tree.build(), Freshness::Fresh, "src/a.ts").to_string();

    assert_eq!(lines_starting_with(&text, "src/dep").len(), 30);
    assert_eq!(lines_starting_with(&text, "pkg").len(), 30);
    assert_eq!(lines_starting_with(&text, "… "), ["… 10 more", "… 10 more"]);
}

#[scenario("queries", "An empty answer says what was searched")]
#[test]
fn an_empty_answer_says_what_was_searched() {
    let index = Tree::new()
        .read_by("ts", "src/x.ts")
        .read_by("python", "tools/p.py")
        .import(
            "src/y.ts",
            Import::new("./x", ImportKind::Static, 2),
            To::Missing(UnresolvedReason::NotFound),
        )
        .padded("ts", 4_000)
        .padded("python", 213)
        .build();

    let text = dependents(&index, Freshness::Fresh, "src/x.ts").to_string();

    assert!(
        text.starts_with("nothing imports src/x.ts among 4,213 files read by ts and python\n"),
        "{text}"
    );
    assert!(
        text.contains("src/y.ts:2 imports ./x (no such file)\n"),
        "{text}"
    );
}

#[test]
fn an_empty_answer_counts_neither_the_files_that_failed_nor_the_files_skipped() {
    let index = Tree::new()
        .read_by("ts", "src/x.ts")
        .failed(
            "src/broken.ts",
            ParseFailure::Panicked {
                message: "boom".to_owned(),
            },
        )
        .failed("dist/bundle.js", ParseFailure::NotUtf8)
        .build();

    let text = dependents(&index, Freshness::Fresh, "src/x.ts").to_string();

    assert!(
        text.starts_with("nothing imports src/x.ts among 1 file read by ts\n"),
        "{text}"
    );
}

#[test]
fn an_empty_impact_says_what_was_searched() {
    let index = Tree::new()
        .calls("client.ts", "http GET /bookings/{}")
        .build();
    let request = ImpactRequest {
        files: vec!["client.ts".to_owned()],
        ..ImpactRequest::default()
    };

    let text = impact(&index, Freshness::Fresh, &request).to_string();

    assert!(
        text.contains("nothing is affected by a change to client.ts among 1 file read by ts\n"),
        "{text}"
    );
}

#[test]
fn a_contract_search_that_finds_nothing_says_what_was_searched() {
    let index = Tree::new().declares("schema.sql", "table bookings").build();

    let text = contract(&index, Freshness::Fresh, "zebra")
        .unwrap()
        .to_string();

    assert!(
        text.starts_with("no contract key contains `zebra` among 1 contract\n"),
        "{text}"
    );
}

#[scenario("queries", "Every answer ends with coverage and freshness")]
#[test]
fn every_answer_ends_with_coverage_and_freshness() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .calls("b.ts", "http GET /x")
        .serves("a.ts", "http GET /x")
        .commit("a1b2c3d4e5", 3)
        .build();
    let request = ImpactRequest {
        files: vec!["a.ts".to_owned()],
        ..ImpactRequest::default()
    };
    let fresh = Freshness::Fresh;

    let answers = [
        dependents(&index, fresh, "a.ts").to_string(),
        dependencies(&index, fresh, "b.ts").to_string(),
        impact(&index, fresh, &request).to_string(),
        path(&index, fresh, "b.ts", "a.ts").to_string(),
        contract(&index, fresh, "http GET /x").unwrap().to_string(),
        coverage(&index, fresh).to_string(),
        status(&index, fresh).to_string(),
    ];

    for text in answers {
        let lines: Vec<&str> = text.lines().collect();
        let [.., coverage_line, freshness_line] = lines.as_slice() else {
            panic!("an answer has at least two lines:\n{text}");
        };
        assert_eq!(*freshness_line, "fresh at a1b2c3d, 3 files dirty", "{text}");
        assert_eq!(
            *coverage_line, "coverage: ts read 2 of 2 files, 100% of imports resolved",
            "{text}"
        );
    }
}

#[scenario("queries", "The resolved share is counted and rounded down")]
#[test]
fn the_footer_gives_the_resolved_share_from_the_counts() {
    // 0.29 * 100.0 is 28.999999999999996 as a float, which floors to 28.
    let index = resolving(29, 100).build();

    let text = dependencies(&index, Freshness::Fresh, "src/a.ts").to_string();

    assert!(
        text.contains("coverage: ts read 1 of 1 files, 29% of imports resolved\n"),
        "{text}"
    );
}

#[test]
fn the_footer_never_rounds_a_lost_import_up_to_all_of_them() {
    let index = resolving(999, 1_000).build();

    let text = dependencies(&index, Freshness::Fresh, "src/a.ts").to_string();

    assert!(
        text.contains("coverage: ts read 1 of 1 files, 99% of imports resolved\n"),
        "{text}"
    );
}

#[test]
fn the_footer_names_only_the_connectors_whose_files_the_answer_involves() {
    let index = Tree::new()
        .read_by("ts", "web/a.ts")
        .read_by("python", "api/p.py")
        .imports("web/b.ts", "web/a.ts")
        .build();

    let text = dependents(&index, Freshness::Fresh, "web/a.ts").to_string();

    assert!(
        text.contains("coverage: ts read 2 of 2 files, 100% of imports resolved\n"),
        "{text}"
    );
    assert!(!text.contains("python"), "{text}");
}

#[test]
fn the_footer_of_an_answer_that_involves_no_file_lists_every_connector() {
    let index = Tree::new()
        .read_by("ts", "web/a.ts")
        .read_by("python", "api/p.py")
        .build();

    let text = dependents(&index, Freshness::Fresh, "nowhere.ts").to_string();

    assert!(
        text.contains(
            "coverage: ts read 1 of 1 files, no imports; python read 1 of 1 files, no imports\n"
        ),
        "{text}"
    );
}

#[test]
fn an_index_without_a_commit_says_only_how_many_files_were_dirty() {
    let index = Tree::new().imports("b.ts", "a.ts").build();

    let text = dependents(&index, Freshness::Fresh, "a.ts").to_string();

    assert!(text.ends_with("fresh, 0 files dirty"), "{text}");
}

#[scenario("queries", "A stale footer says so")]
#[test]
fn a_stale_footer_says_so() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .commit("a1b2c3d4e5", 3)
        .build();
    let stale = Freshness::Stale {
        added: 0,
        removed: 0,
        changed: 2,
    };

    let text = dependents(&index, stale, "a.ts").to_string();

    let last = text.lines().last().unwrap();
    assert!(last.starts_with("stale:"), "{last}");
    assert!(last.contains("2 files changed since the index"), "{last}");
}
