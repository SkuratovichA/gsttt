//! `impact`: what a change reaches through imports and contracts.

use std::collections::VecDeque;

use gsttt_core::contract::Role;
use gsttt_core::fact::{FileTags, Import, ImportKind, UnresolvedReason};
use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{Affected, Answer, Entries, Impact, ImpactRequest, impact};
use gsttt_core::testing::{To, Tree, file};
use gsttt_scenario::scenario;
use proptest::prelude::*;
use serde_json::{json, to_value};

/// What an empty list of paths is compared with, because clippy asks `assert_eq!` over `is_empty` in a test.
const NONE: [&str; 0] = [];

fn request(files: &[&str]) -> ImpactRequest {
    ImpactRequest {
        files: files.iter().map(|path| (*path).to_owned()).collect(),
        ..ImpactRequest::default()
    }
}

fn run(index: &Index, files: &[&str]) -> Impact {
    impact(index, Freshness::Fresh, &request(files))
}

/// Path, distance and reason of each entry, which is what a reader of the answer sees.
fn rows(list: &[Affected]) -> Vec<(&str, u32, String)> {
    list.iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.distance,
                entry.reason.to_string(),
            )
        })
        .collect()
}

fn paths(list: &[Affected]) -> Vec<&str> {
    list.iter().map(|entry| entry.path.as_str()).collect()
}

fn chain() -> Index {
    Tree::new()
        .imports("c.ts", "b.ts")
        .imports("b.ts", "a.ts")
        .build()
}

fn bookings() -> Tree {
    Tree::new()
        .read_by("openapi", "openapi.yaml")
        .declares("openapi.yaml", "http GET /bookings/{}")
        .serves("bookings.controller.ts", "http GET /bookings/{}")
        .calls("client.ts", "http GET /bookings/{}")
}

#[scenario("queries", "A change reaches importers of importers")]
#[test]
fn a_change_reaches_importers_of_importers() {
    let answer = run(&chain(), &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("b.ts", 1, "imports a.ts".to_owned()),
            ("c.ts", 2, "imports b.ts".to_owned())
        ]
    );
    assert_eq!(paths(&answer.tests), NONE);
    assert_eq!(answer.changed, ["a.ts"]);
}

#[scenario("queries", "A mod declaration carries no impact")]
#[test]
fn a_mod_declaration_carries_no_impact() {
    let index = Tree::new()
        .read_by("rust", "lib.rs")
        .read_by("rust", "store.rs")
        .import(
            "lib.rs",
            Import::new("store", ImportKind::Module, 1),
            file("store.rs"),
        )
        .build();

    let answer = run(&index, &["store.rs"]);

    assert_eq!(paths(&answer.affected), NONE);
    assert_eq!(paths(&answer.tests), NONE);
}

#[test]
fn a_use_beside_the_mod_declaration_carries_impact() {
    let index = Tree::new()
        .read_by("rust", "lib.rs")
        .read_by("rust", "store.rs")
        .import(
            "lib.rs",
            Import::new("store", ImportKind::Module, 1),
            file("store.rs"),
        )
        .import(
            "lib.rs",
            Import::new("store::open", ImportKind::Static, 2),
            file("store.rs"),
        )
        .build();

    let answer = run(&index, &["store.rs"]);

    assert_eq!(
        rows(&answer.affected),
        [("lib.rs", 1, "imports store.rs".to_owned())]
    );
}

#[scenario("queries", "Depth limits the walk")]
#[test]
fn depth_limits_the_walk() {
    let limited = ImpactRequest {
        depth: Some(1),
        ..request(&["a.ts"])
    };

    let answer = impact(&chain(), Freshness::Fresh, &limited);

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert_eq!(answer.depth, Some(1));
    assert!(
        answer
            .to_string()
            .contains("impact of a.ts, within 1 hop\n")
    );
}

#[test]
fn depth_zero_reaches_nothing() {
    let limited = ImpactRequest {
        depth: Some(0),
        ..request(&["a.ts"])
    };

    let answer = impact(&chain(), Freshness::Fresh, &limited);

    assert_eq!(paths(&answer.affected), NONE);
}

#[scenario("queries", "Import cycles terminate")]
#[test]
fn import_cycles_terminate() {
    let index = Tree::new()
        .imports("a.ts", "b.ts")
        .imports("b.ts", "a.ts")
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("b.ts", 1, "imports a.ts".to_owned())]
    );
}

#[scenario("queries", "Changing a schema reaches servers and callers")]
#[test]
fn changing_a_schema_reaches_servers_and_callers() {
    let index = bookings().build();

    let answer = run(&index, &["openapi.yaml"]);

    assert_eq!(
        rows(&answer.affected),
        [
            (
                "bookings.controller.ts",
                1,
                "serves http GET /bookings/{}".to_owned()
            ),
            ("client.ts", 1, "calls http GET /bookings/{}".to_owned()),
        ]
    );
}

#[scenario("queries", "Changing a server reaches its callers only")]
#[test]
fn changing_a_server_reaches_its_callers_only() {
    let index = bookings().build();

    let answer = run(&index, &["bookings.controller.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("client.ts", 1, "calls http GET /bookings/{}".to_owned())]
    );
}

#[scenario("queries", "Changing a caller reaches no server")]
#[test]
fn changing_a_caller_reaches_no_server() {
    let index = bookings().build();

    let answer = run(&index, &["client.ts"]);

    assert_eq!(paths(&answer.affected), NONE);
    assert_eq!(paths(&answer.tests), NONE);
    assert!(answer.to_string().contains(
        "nothing is affected by a change to client.ts among 3 files read by openapi and ts"
    ));
}

#[scenario("queries", "A call reaches a route through a parameter")]
#[test]
fn a_call_reaches_a_route_through_a_parameter() {
    let index = Tree::new()
        .serves("bookings.controller.ts", "http GET /bookings/{}")
        .calls("app.ts", "http GET /bookings/me")
        .build();

    let answer = run(&index, &["bookings.controller.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("app.ts", 1, "calls http GET /bookings/me".to_owned())]
    );
}

#[test]
fn a_prefix_joins_a_call_to_a_route_it_is_written_before() {
    let index = Tree::new()
        .serves("bookings.controller.ts", "http GET /bookings/{}")
        .calls("app.ts", "http GET /api/bookings/me")
        .prefixes(&["/api"])
        .build();

    let answer = run(&index, &["bookings.controller.ts"]);

    assert_eq!(paths(&answer.affected), ["app.ts"]);
}

#[test]
fn contracts_of_other_kinds_meet_by_equality() {
    let index = Tree::new()
        .declares("schema.prisma", "table bookings")
        .calls("repo.ts", "table bookings")
        .calls("other.ts", "table booking_items")
        .build();

    let answer = run(&index, &["schema.prisma"]);

    assert_eq!(
        rows(&answer.affected),
        [("repo.ts", 1, "calls table bookings".to_owned())]
    );
}

#[scenario("queries", "Contract and import hops chain")]
#[test]
fn contract_and_import_hops_chain() {
    let index = Tree::new()
        .serves("bookings.controller.ts", "http GET /bookings/{}")
        .calls("api.ts", "http GET /bookings/{}")
        .imports("page.tsx", "api.ts")
        .build();

    let answer = run(&index, &["bookings.controller.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("api.ts", 1, "calls http GET /bookings/{}".to_owned()),
            ("page.tsx", 2, "imports api.ts".to_owned())
        ]
    );
}

#[test]
fn a_declared_contract_is_reached_through_an_import_of_its_schema() {
    let index = Tree::new()
        .declares("schema.ts", "http GET /bookings/{}")
        .imports("schema.ts", "types.ts")
        .calls("client.ts", "http GET /bookings/{}")
        .build();

    let answer = run(&index, &["types.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("schema.ts", 1, "imports types.ts".to_owned()),
            ("client.ts", 2, "calls http GET /bookings/{}".to_owned())
        ]
    );
}

#[scenario("queries", "Tests are listed apart")]
#[test]
fn tests_are_listed_apart() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .imports("a.test.ts", "a.ts")
        .tag("a.test.ts", FileTags::TEST)
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert_eq!(paths(&answer.tests), ["a.test.ts"]);
    let text = answer.to_string();
    let affected = text.find("affected files (1):").unwrap();
    let tests = text.find("tests (1):").unwrap();
    assert!(affected < tests && text.contains("a.test.ts  distance 1  imports a.ts"));
}

#[test]
fn files_are_ordered_by_distance_and_then_by_path() {
    let index = Tree::new()
        .imports("z.ts", "a.ts")
        .imports("m.ts", "a.ts")
        .imports("b.ts", "m.ts")
        .imports("a2.ts", "z.ts")
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("m.ts", 1, "imports a.ts".to_owned()),
            ("z.ts", 1, "imports a.ts".to_owned()),
            ("a2.ts", 2, "imports z.ts".to_owned()),
            ("b.ts", 2, "imports m.ts".to_owned()),
        ]
    );
}

#[test]
fn several_changed_files_are_walked_together() {
    let index = Tree::new()
        .imports("c.ts", "a.ts")
        .imports("d.ts", "b.ts")
        .imports("d.ts", "c.ts")
        .build();

    let answer = run(&index, &["a.ts", "b.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("c.ts", 1, "imports a.ts".to_owned()),
            ("d.ts", 1, "imports b.ts".to_owned()),
        ]
    );
    assert_eq!(answer.changed, ["a.ts", "b.ts"]);
}

#[scenario("queries", "The reason comes from the file with the smallest path")]
#[test]
fn the_reason_comes_from_the_file_with_the_smallest_path() {
    // `w.ts` is reached from both changed files, `x.ts` from both of the files at distance 1 that it imports. `z.ts` is found before `y.ts`, because the file it imports is the smaller one, so the order of discovery is not the order of paths.
    let index = Tree::new()
        .imports("w.ts", "a.ts")
        .imports("w.ts", "b.ts")
        .imports("z.ts", "a.ts")
        .imports("y.ts", "b.ts")
        .imports("x.ts", "y.ts")
        .imports("x.ts", "z.ts")
        .build();

    // Asked in the opposite order, so that the order of the request is no part of the answer.
    let answer = run(&index, &["b.ts", "a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("w.ts", 1, "imports a.ts".to_owned()),
            ("y.ts", 1, "imports b.ts".to_owned()),
            ("z.ts", 1, "imports a.ts".to_owned()),
            ("x.ts", 2, "imports y.ts".to_owned()),
        ]
    );
}

#[scenario("queries", "From one file an import comes before a contract")]
#[test]
fn from_one_file_an_import_comes_before_a_contract() {
    let index = Tree::new()
        .serves("lib.ts", "http GET /x")
        .imports("x.ts", "lib.ts")
        .calls("x.ts", "http GET /x")
        .build();

    let answer = run(&index, &["lib.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("x.ts", 1, "imports lib.ts".to_owned())]
    );
}

#[test]
fn a_smaller_path_comes_before_an_import_from_a_larger_one() {
    // `x.ts` is reached at distance 1 from `a.ts` by a contract and from `b.ts` by an import: the file comes first, and an import is not preferred over every contract of the walk, only over those of the same file.
    let index = Tree::new()
        .serves("a.ts", "http GET /x")
        .calls("x.ts", "http GET /x")
        .imports("x.ts", "b.ts")
        .build();

    let answer = run(&index, &["b.ts", "a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("x.ts", 1, "calls http GET /x".to_owned())]
    );
}

#[test]
fn from_one_file_contracts_come_in_key_order() {
    let index = Tree::new()
        .serves("lib.ts", "queue beta")
        .serves("lib.ts", "queue alpha")
        .calls("x.ts", "queue beta")
        .calls("x.ts", "queue alpha")
        .build();

    let answer = run(&index, &["lib.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("x.ts", 1, "calls queue alpha".to_owned())]
    );
}

#[scenario("queries", "A file's first site by line gives the reason")]
#[test]
fn a_file_that_serves_and_calls_a_contract_gives_its_first_site_by_line() {
    // Each file has both sites, in opposite order of line, so that neither the role order nor the order the sites were written in decides.
    let index = Tree::new()
        .declares("schema.yaml", "http GET /x")
        .site("early-server.ts", "http GET /x", Role::Calls, 9)
        .site("early-server.ts", "http GET /x", Role::Serves, 3)
        .site("early-caller.ts", "http GET /x", Role::Serves, 9)
        .site("early-caller.ts", "http GET /x", Role::Calls, 3)
        .build();

    let answer = run(&index, &["schema.yaml"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("early-caller.ts", 1, "calls http GET /x".to_owned()),
            ("early-server.ts", 1, "serves http GET /x".to_owned()),
        ]
    );
}

#[test]
fn a_server_reaches_a_file_that_serves_and_calls_by_its_call() {
    let index = Tree::new()
        .serves("server.ts", "http GET /x")
        .site("proxy.ts", "http GET /x", Role::Serves, 3)
        .site("proxy.ts", "http GET /x", Role::Calls, 9)
        .build();

    let answer = run(&index, &["server.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("proxy.ts", 1, "calls http GET /x".to_owned())]
    );
}

#[test]
fn a_changed_file_that_serves_and_calls_a_contract_is_a_server() {
    let index = Tree::new()
        .site("proxy.ts", "http GET /x", Role::Serves, 3)
        .site("proxy.ts", "http GET /x", Role::Calls, 9)
        .serves("other-server.ts", "http GET /x")
        .calls("client.ts", "http GET /x")
        .build();

    let answer = run(&index, &["proxy.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [("client.ts", 1, "calls http GET /x".to_owned())]
    );
}

#[test]
fn a_declarer_reached_after_a_server_still_reaches_the_other_servers() {
    // `server.ts` crosses the contract first, as a server, and reaches only its caller. `schema.ts` imports it and declares the contract, so it crosses the same contract again, as a declarer, and reaches the other server.
    let index = Tree::new()
        .imports("server.ts", "lib.ts")
        .serves("server.ts", "http GET /x")
        .imports("schema.ts", "server.ts")
        .declares("schema.ts", "http GET /x")
        .serves("other-server.ts", "http GET /x")
        .calls("client.ts", "http GET /x")
        .build();

    let answer = run(&index, &["lib.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            ("server.ts", 1, "imports lib.ts".to_owned()),
            ("client.ts", 2, "calls http GET /x".to_owned()),
            ("schema.ts", 2, "imports server.ts".to_owned()),
            ("other-server.ts", 3, "serves http GET /x".to_owned()),
        ]
    );
}

#[test]
fn a_changed_file_is_never_listed_as_affected() {
    let index = chain();

    let answer = run(&index, &["a.ts", "b.ts"]);

    assert_eq!(paths(&answer.affected), ["c.ts"]);
}

#[test]
fn a_path_is_found_however_it_is_written() {
    let answer = run(&chain(), &["./a.ts", "a.ts"]);

    assert_eq!(answer.changed, ["a.ts"]);
    assert_eq!(paths(&answer.affected), ["b.ts", "c.ts"]);
}

#[scenario("queries", "A deleted file is listed, not traced")]
#[test]
fn a_deleted_file_is_listed_not_traced() {
    let index = Tree::new()
        .import(
            "src/a.ts",
            Import::new("./old", ImportKind::Static, 4),
            To::Missing(UnresolvedReason::NotFound),
        )
        .import(
            "src/a.ts",
            Import::new("./older", ImportKind::Static, 5),
            To::Missing(UnresolvedReason::NotFound),
        )
        .build();
    let deleted = ImpactRequest {
        deleted: vec!["src/old.ts".to_owned()],
        ..ImpactRequest::default()
    };

    let answer = impact(&index, Freshness::Fresh, &deleted);

    assert!(answer.changed.is_empty() && answer.affected.is_empty());
    assert_eq!(answer.unknown, NONE);
    let [listed] = answer.deleted.as_slice() else {
        panic!("one deleted file, got {:?}", answer.deleted);
    };
    assert_eq!(listed.path, "src/old.ts");
    let [miss] = listed.possible_misses.as_slice() else {
        panic!("one possible miss, got {:?}", listed.possible_misses);
    };
    assert_eq!(
        (miss.path.as_str(), miss.line, miss.spec.as_str()),
        ("src/a.ts", 4, "./old")
    );
    let text = answer.to_string();
    assert!(text.contains("src/old.ts is deleted, so it is not traced"));
    assert!(text.contains("src/a.ts:4 imports ./old (no such file)"));
}

#[test]
fn a_deleted_file_the_index_still_holds_is_traced() {
    let answer = impact(
        &chain(),
        Freshness::Stale {
            added: 0,
            removed: 1,
            changed: 0,
        },
        &ImpactRequest {
            deleted: vec!["a.ts".to_owned()],
            ..ImpactRequest::default()
        },
    );

    assert_eq!(answer.deleted, Vec::new());
    assert_eq!(paths(&answer.affected), ["b.ts", "c.ts"]);
}

#[scenario("queries", "An unknown path is named")]
#[test]
fn an_unknown_path_is_named() {
    let index = Tree::new()
        .imports("a.ts", "b.ts")
        .unread("notes.txt")
        .build();

    let answer = run(&index, &["notes.txt", "missing.ts"]);

    assert_eq!(answer.unknown, ["notes.txt", "missing.ts"]);
    assert!(answer.changed.is_empty() && answer.deleted.is_empty());
    let text = answer.to_string();
    assert!(text.contains("notes.txt is not in the index\n"));
    assert!(text.contains("missing.ts is not in the index\n"));
    assert!(!text.contains("nothing is affected"));
}

#[test]
fn an_absent_path_is_listed_once_in_the_order_it_was_first_asked() {
    let strings =
        |paths: &[&str]| -> Vec<String> { paths.iter().map(|path| (*path).to_owned()).collect() };

    let answer = impact(
        &chain(),
        Freshness::Fresh,
        &ImpactRequest {
            files: strings(&["z.md", "a.md", "./z.md", "m.md", "a.md"]),
            deleted: strings(&["m.md", "gone.ts", "./gone.ts"]),
            depth: None,
        },
    );

    // `m.md` was asked as changed and then reported deleted: the report wins, and it keeps its first place.
    assert_eq!(answer.unknown, ["z.md", "a.md"]);
    let deleted: Vec<&str> = answer
        .deleted
        .iter()
        .map(|deleted| deleted.path.as_str())
        .collect();
    assert_eq!(deleted, ["m.md", "gone.ts"]);
}

#[test]
fn a_request_with_no_file_still_answers() {
    let answer = run(&chain(), &[]);

    assert!(answer.to_string().starts_with("impact of no files\n"));
}

#[test]
fn the_text_names_the_change_its_reach_and_the_footer() {
    let index = chain();

    let text = run(&index, &["a.ts"]).to_string();

    assert_eq!(
        text,
        "impact of a.ts\n\
         affected files (2):\n\
         b.ts  distance 1  imports a.ts\n\
         c.ts  distance 2  imports b.ts\n\
         coverage: ts read 3 of 3 files, 100% of imports resolved\n\
         fresh, 0 files dirty"
    );
    assert_eq!(run(&index, &["a.ts"]).text(Entries::All), text);
}

fn reverse_distances(edges: &[(usize, usize)], size: usize, changed: usize) -> Vec<Option<u32>> {
    let mut distance = vec![None; size];
    distance[changed] = Some(0);
    let mut queue = VecDeque::from([changed]);
    while let Some(at) = queue.pop_front() {
        let next = distance[at].map(|hops| hops + 1);
        for &(from, to) in edges {
            if to == at && distance[from].is_none() {
                distance[from] = next;
                queue.push_back(from);
            }
        }
    }
    distance
}

proptest! {
    /// On any import graph, impact lists exactly the files that reach a changed file, each at its shortest distance.
    #[test]
    fn impact_is_the_reverse_closure_of_imports(
        edges in prop::collection::vec((0usize..8, 0usize..8), 0..24),
        changed in 0usize..8,
    ) {
        let name = |number: usize| format!("f{number}.ts");
        let mut tree = Tree::new();
        for number in 0..8 {
            tree = tree.read_by("ts", &name(number));
        }
        for &(from, to) in &edges {
            tree = tree.imports(&name(from), &name(to));
        }
        let index = tree.build();

        let answer = run(&index, &[&name(changed)]);

        let distances = reverse_distances(&edges, 8, changed);
        let mut expected: Vec<(String, u32)> = (0..8)
            .filter(|&number| number != changed)
            .filter_map(|number| distances[number].map(|hops| (name(number), hops)))
            .collect();
        expected.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
        let actual: Vec<(String, u32)> = answer
            .affected
            .iter()
            .map(|entry| (entry.path.clone(), entry.distance))
            .collect();
        prop_assert_eq!(actual, expected);
    }
}

/// A row of `rows`, with the reason written out.
fn row<'a>(path: &'a str, distance: u32, reason: &str) -> (&'a str, u32, String) {
    (path, distance, reason.to_owned())
}

#[scenario("queries", "A changed barrel reaches every importer")]
#[test]
fn a_changed_barrel_reaches_every_importer() {
    let index = Tree::new()
        .exports("index.ts", &["own"])
        .re_exports_all("index.ts", "a.ts")
        .exports("a.ts", &["fromA"])
        .imports_names("form.ts", "index.ts", &["fromA"])
        .imports_names("page.ts", "index.ts", &["own"])
        .build();

    let answer = run(&index, &["index.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("form.ts", 1, "imports fromA from index.ts"),
            row("page.ts", 1, "imports own from index.ts"),
        ]
    );
}

#[scenario(
    "queries",
    "A file that uses an affected name is affected in all its names"
)]
#[test]
fn a_file_that_uses_an_affected_name_is_affected_in_all_its_names() {
    let index = Tree::new()
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "a.ts")
        .re_exports_all("index.ts", "b.ts")
        .exports("a.ts", &["fromA"])
        .exports("b.ts", &["fromB"])
        .imports_names("b.ts", "a.ts", &["fromA"])
        .imports_names("page.ts", "index.ts", &["fromB"])
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("b.ts", 1, "imports fromA from a.ts"),
            row("index.ts", 1, "re-exports fromA from a.ts"),
            row("page.ts", 3, "imports fromB from index.ts"),
        ]
    );
}

#[scenario("queries", "A name the barrel does not export is affected")]
#[test]
fn a_name_the_barrel_does_not_export_is_affected() {
    let index = Tree::new()
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "a.ts")
        .exports("a.ts", &["kept"])
        .imports_names("old.ts", "index.ts", &["gone"])
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports kept from a.ts"),
            row("old.ts", 2, "imports gone from index.ts"),
        ]
    );
}

/// A barrel of `fromA` and `fromB`, and a page for each name.
fn two_sources() -> Tree {
    Tree::new()
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "a.ts")
        .re_exports_all("index.ts", "b.ts")
        .exports("a.ts", &["fromA"])
        .exports("b.ts", &["fromB"])
        .imports_names("form.ts", "index.ts", &["fromA"])
        .imports_names("page.ts", "index.ts", &["fromB"])
}

#[scenario("queries", "A barrel passes on only what it re-exports")]
#[test]
fn a_barrel_passes_on_only_what_it_re_exports() {
    let answer = run(&two_sources().build(), &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports fromA from a.ts"),
            row("form.ts", 2, "imports fromA from index.ts"),
        ]
    );
    assert!(!paths(&answer.affected).contains(&"page.ts"));
}

#[scenario("queries", "A named re-export passes on its alias")]
#[test]
fn a_named_re_export_passes_on_its_alias() {
    let index = Tree::new()
        .exports("index.ts", &["other", "useBar"])
        .exports("foo.ts", &["useFoo"])
        .re_exports("index.ts", "foo.ts", &[("useBar", "useFoo")])
        .re_exports("index.ts", "other.ts", &[("other", "other")])
        .imports_names("x.ts", "index.ts", &["useBar"])
        .imports_names("y.ts", "index.ts", &["other"])
        .build();

    let answer = run(&index, &["foo.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports useBar from foo.ts"),
            row("x.ts", 2, "imports useBar from index.ts"),
        ]
    );
}

#[scenario("queries", "Chained barrels pass names on")]
#[test]
fn chained_barrels_pass_names_on() {
    let index = Tree::new()
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "inner.ts")
        .exports("inner.ts", &[])
        .re_exports_all("inner.ts", "a.ts")
        .re_exports_all("inner.ts", "b.ts")
        .exports("a.ts", &["fromA"])
        .exports("b.ts", &["fromB"])
        .imports_names("app.ts", "index.ts", &["fromA"])
        .imports_names("other.ts", "index.ts", &["fromB"])
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("inner.ts", 1, "re-exports fromA from a.ts"),
            row("index.ts", 2, "re-exports fromA from inner.ts"),
            row("app.ts", 3, "imports fromA from index.ts"),
        ]
    );
}

#[scenario("queries", "A namespace import takes every name")]
#[test]
fn a_namespace_import_takes_every_name() {
    let index = Tree::new()
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "a.ts")
        .exports("a.ts", &["fromA"])
        .imports_namespace("all.ts", "index.ts")
        .imports_for_effect("side.ts", "index.ts")
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports fromA from a.ts"),
            row("all.ts", 2, "imports index.ts"),
            row("side.ts", 2, "imports index.ts"),
        ]
    );
}

#[scenario("queries", "Unknown exports keep every importer")]
#[test]
fn unknown_exports_keep_every_importer() {
    let index = Tree::new()
        .re_exports_all("index.ts", "legacy.js")
        .imports_names("page.ts", "index.ts", &["thing"])
        .build();

    let answer = run(&index, &["legacy.js"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "imports legacy.js"),
            row("page.ts", 2, "imports index.ts"),
        ]
    );
}

#[scenario("queries", "A barrel reached for some names crosses no contract")]
#[test]
fn a_barrel_reached_for_some_names_crosses_no_contract() {
    let index = Tree::new()
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "a.ts")
        .exports("a.ts", &["fromA"])
        .serves("index.ts", "http GET /health")
        .calls("client.ts", "http GET /health")
        .build();

    let by_name = run(&index, &["a.ts"]);
    let whole = run(&index, &["index.ts"]);

    assert_eq!(paths(&by_name.affected), ["index.ts"]);
    assert_eq!(paths(&whole.affected), ["client.ts"]);
}

#[scenario("queries", "Names are cut after three")]
#[test]
fn names_are_cut_after_three() {
    let index = Tree::new()
        .exports("a.ts", &["four", "one", "three", "two"])
        .imports_names("x.ts", "a.ts", &["one", "two", "three", "four"])
        .imports_names("y.ts", "a.ts", &["one"])
        .build();
    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("x.ts", 1, "imports four, one, three and 1 more from a.ts"),
            row("y.ts", 1, "imports one from a.ts"),
        ]
    );
    assert!(
        answer
            .text(Entries::All)
            .contains("x.ts  distance 1  imports four, one, three, two from a.ts")
    );
    assert_eq!(
        to_value(&answer.affected[0].reason).unwrap(),
        json!({"via": "imports", "file": "a.ts", "names": ["four", "one", "three", "two"]})
    );
}

#[test]
fn a_re_export_is_written_as_re_exports_in_json() {
    let answer = run(&two_sources().build(), &["a.ts"]);

    assert_eq!(
        to_value(&answer.affected[0].reason).unwrap(),
        json!({"via": "re-exports", "file": "a.ts", "names": ["fromA"]})
    );
}

#[test]
fn a_cycle_of_star_exports_ends_and_knows_the_names_of_every_member() {
    let index = Tree::new()
        .exports("s.ts", &["fromS"])
        .exports("p.ts", &["ownP"])
        .exports("q.ts", &["ownQ"])
        .re_exports_all("p.ts", "s.ts")
        .re_exports_all("p.ts", "q.ts")
        .re_exports_all("q.ts", "p.ts")
        .imports_names("x.ts", "q.ts", &["fromS"])
        .imports_names("y.ts", "q.ts", &["ownP"])
        .imports_names("z.ts", "q.ts", &["missing"])
        .build();

    let answer = run(&index, &["s.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("p.ts", 1, "re-exports fromS from s.ts"),
            row("q.ts", 2, "re-exports fromS from p.ts"),
            row("x.ts", 3, "imports fromS from q.ts"),
            row("z.ts", 3, "imports missing from q.ts"),
        ]
    );
}

#[test]
fn a_barrel_whose_names_grow_over_two_layers_passes_each_gain_on_once() {
    let index = two_sources()
        .imports_names("b.ts", "a.ts", &["fromA"])
        .imports_names("both.ts", "index.ts", &["fromA", "fromB"])
        .build();

    let answer = run(&index, &["a.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("b.ts", 1, "imports fromA from a.ts"),
            row("index.ts", 1, "re-exports fromA from a.ts"),
            row("both.ts", 2, "imports fromA from index.ts"),
            row("form.ts", 2, "imports fromA from index.ts"),
            row("page.ts", 3, "imports fromB from index.ts"),
        ]
    );
}

#[scenario(
    "queries",
    "A file that exports nothing more still reaches the importers of its barrel"
)]
#[test]
fn a_file_that_exports_nothing_more_still_reaches_the_importers_of_its_barrel() {
    let index = Tree::new()
        .exports("only.ts", &[])
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "only.ts")
        .imports_names("page.ts", "index.ts", &["foo"])
        .build();

    let answer = run(&index, &["only.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports only.ts"),
            row("page.ts", 2, "imports foo from index.ts"),
        ]
    );
}

#[scenario(
    "queries",
    "A file left with only a default export still reaches the importers of its barrel"
)]
#[test]
fn a_file_left_with_only_a_default_export_still_reaches_the_importers_of_its_barrel() {
    let index = Tree::new()
        .exports("only.ts", &["default"])
        .exports("index.ts", &[])
        .re_exports_all("index.ts", "only.ts")
        .imports_names("page.ts", "index.ts", &["foo"])
        .build();

    let answer = run(&index, &["only.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports only.ts"),
            row("page.ts", 2, "imports foo from index.ts"),
        ]
    );
}

#[scenario(
    "queries",
    "A star barrel behind a star barrel reaches the importers of both"
)]
#[test]
fn a_star_barrel_behind_a_star_barrel_reaches_the_importers_of_both() {
    let index = Tree::new()
        .exports("only.ts", &[])
        .exports("mid.ts", &[])
        .exports("index.ts", &[])
        .re_exports_all("mid.ts", "only.ts")
        .re_exports_all("index.ts", "mid.ts")
        .imports_names("page.ts", "index.ts", &["foo"])
        .build();

    let answer = run(&index, &["only.ts"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("mid.ts", 1, "re-exports only.ts"),
            row("index.ts", 2, "re-exports mid.ts"),
            row("page.ts", 3, "imports foo from index.ts"),
        ]
    );
}

#[scenario(
    "queries",
    "A named import of a file with unknown exports is followed at file level"
)]
#[test]
fn a_named_import_of_a_file_with_unknown_exports_is_followed_at_file_level() {
    // Python and Rust record the names an import takes and list no exports.
    let named = Tree::new()
        .imports_names("user.py", "lib.py", &["parse"])
        .build();
    let plain = Tree::new().imports("user.py", "lib.py").build();

    let by_name = run(&named, &["lib.py"]);

    assert_eq!(
        rows(&by_name.affected),
        [row("user.py", 1, "imports lib.py")]
    );
    assert_eq!(by_name.affected, run(&plain, &["lib.py"]).affected);
    assert_eq!(
        to_value(&by_name.affected).unwrap(),
        to_value(&run(&plain, &["lib.py"]).affected).unwrap()
    );
}

#[scenario(
    "queries",
    "A star re-export of a file with unknown exports is followed at file level"
)]
#[test]
fn a_star_re_export_of_a_file_with_unknown_exports_is_followed_at_file_level() {
    let star = Tree::new()
        .re_exports_all("index.py", "lib.py")
        .imports_names("app.py", "index.py", &["parse"])
        .build();
    let plain = Tree::new()
        .imports("index.py", "lib.py")
        .imports("app.py", "index.py")
        .build();

    let by_name = run(&star, &["lib.py"]);

    assert_eq!(
        rows(&by_name.affected),
        [
            row("index.py", 1, "imports lib.py"),
            row("app.py", 2, "imports index.py"),
        ]
    );
    assert_eq!(by_name.affected, run(&plain, &["lib.py"]).affected);
    assert_eq!(
        to_value(&by_name.affected).unwrap(),
        to_value(&run(&plain, &["lib.py"]).affected).unwrap()
    );
}

#[scenario(
    "queries",
    "A named re-export of a file with unknown exports is followed at file level"
)]
#[test]
fn a_named_re_export_of_a_file_with_unknown_exports_is_followed_at_file_level() {
    let named = Tree::new()
        .exports("index.ts", &["thing"])
        .re_exports("index.ts", "legacy.js", &[("thing", "thing")])
        .imports_names("page.ts", "index.ts", &["thing"])
        .build();
    let plain = Tree::new()
        .exports("index.ts", &["thing"])
        .imports("index.ts", "legacy.js")
        .imports_names("page.ts", "index.ts", &["thing"])
        .build();

    let by_name = run(&named, &["legacy.js"]);

    assert_eq!(
        rows(&by_name.affected),
        [
            row("index.ts", 1, "imports legacy.js"),
            row("page.ts", 2, "imports thing from index.ts"),
        ]
    );
    assert_eq!(by_name.affected, run(&plain, &["legacy.js"]).affected);
    assert_eq!(
        to_value(&by_name.affected).unwrap(),
        to_value(&run(&plain, &["legacy.js"]).affected).unwrap()
    );
}

#[scenario(
    "queries",
    "A barrel that re-exports a file with unknown exports has unknown exports too"
)]
#[test]
fn a_barrel_that_re_exports_a_file_with_unknown_exports_has_unknown_exports_too() {
    let index = Tree::new()
        .exports("index.ts", &["own"])
        .re_exports_all("index.ts", "legacy.js")
        .imports_names("page.ts", "index.ts", &["thing"])
        .build();

    let answer = run(&index, &["legacy.js"]);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "imports legacy.js"),
            row("page.ts", 2, "imports index.ts"),
        ]
    );
}
