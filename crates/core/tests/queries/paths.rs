//! `path`: the shortest chain of imports and contract hops between two files.

use std::collections::VecDeque;

use gsttt_core::contract::Role;
use gsttt_core::fact::{FileTags, Import, ImportKind};
use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{Link, Route, path};
use gsttt_scenario::scenario;
use proptest::prelude::*;

use crate::fixture::{Tree, file};

fn route(index: &Index, from: &str, to: &str) -> Route {
    path(index, Freshness::Fresh, from, to)
}

/// Each file of the chain with the words that say how the chain got there.
fn steps(route: &Route) -> Vec<(&str, Option<String>)> {
    route
        .steps
        .iter()
        .map(|step| {
            (
                step.path.as_str(),
                step.by.as_ref().map(ToString::to_string),
            )
        })
        .collect()
}

fn files(route: &Route) -> Vec<&str> {
    route.steps.iter().map(|step| step.path.as_str()).collect()
}

#[scenario("queries", "A chain runs through a contract")]
#[test]
fn a_chain_runs_through_a_contract() {
    let index = Tree::new()
        .imports("page.tsx", "api.ts")
        .calls("api.ts", "http GET /bookings/{}")
        .serves("bookings.controller.ts", "http GET /bookings/{}")
        .build();

    let answer = route(&index, "page.tsx", "bookings.controller.ts");

    assert_eq!(
        steps(&answer),
        [
            ("page.tsx", None),
            ("api.ts", Some("by import".to_owned())),
            (
                "bookings.controller.ts",
                Some("by http GET /bookings/{}".to_owned())
            ),
        ]
    );
    let text = answer.to_string();
    assert!(
        text.starts_with(
            "path from page.tsx to bookings.controller.ts, 2 hops\n\
             page.tsx\n\
             api.ts  by import\n\
             bookings.controller.ts  by http GET /bookings/{}\n"
        ),
        "{text}"
    );
}

#[scenario("queries", "No chain is reported with its reach")]
#[test]
fn no_chain_is_reported_with_its_reach() {
    let index = Tree::new()
        .imports("a.ts", "b.ts")
        .imports("a.ts", "c.ts")
        .imports("c.ts", "d.ts")
        .imports("e.ts", "f.ts")
        .build();

    let answer = route(&index, "a.ts", "f.ts");

    assert_eq!(files(&answer), Vec::<&str>::new());
    assert_eq!(answer.reach, Some(3));
    let text = answer.to_string();
    assert!(
        text.starts_with("no chain from a.ts to f.ts: a.ts reaches 3 files\n"),
        "{text}"
    );
}

#[test]
fn a_file_that_reaches_nothing_says_so() {
    let index = Tree::new().imports("a.ts", "b.ts").build();

    let answer = route(&index, "b.ts", "a.ts");

    assert_eq!(answer.reach, Some(0));
    assert!(
        answer
            .to_string()
            .starts_with("no chain from b.ts to a.ts: b.ts reaches 0 files\n")
    );
}

#[test]
fn a_chain_follows_imports_and_not_importers() {
    let index = Tree::new()
        .imports("c.ts", "b.ts")
        .imports("b.ts", "a.ts")
        .build();

    assert_eq!(
        files(&route(&index, "c.ts", "a.ts")),
        ["c.ts", "b.ts", "a.ts"]
    );
    assert_eq!(files(&route(&index, "a.ts", "c.ts")), Vec::<&str>::new());
}

#[test]
fn the_shortest_of_several_chains_wins() {
    let index = Tree::new()
        .imports("a.ts", "b.ts")
        .imports("b.ts", "c.ts")
        .imports("c.ts", "d.ts")
        .imports("a.ts", "x.ts")
        .imports("x.ts", "d.ts")
        .build();

    assert_eq!(
        files(&route(&index, "a.ts", "d.ts")),
        ["a.ts", "x.ts", "d.ts"]
    );
}

#[scenario("queries", "A chain comes from the file with the smallest path")]
#[test]
fn of_equally_short_chains_the_one_through_the_smallest_path_wins() {
    let index = Tree::new()
        .imports("start.ts", "b.ts")
        .imports("start.ts", "a.ts")
        .imports("b.ts", "goal.ts")
        .imports("a.ts", "goal.ts")
        .build();

    assert_eq!(
        files(&route(&index, "start.ts", "goal.ts")),
        ["start.ts", "a.ts", "goal.ts"]
    );
}

#[test]
fn a_smaller_path_comes_before_an_import_from_a_larger_one() {
    // `z.ts` is found by an import and `a.ts` by a contract, both from `start.ts`, so `z.ts` is found first. The
    // chain still goes through `a.ts`, because the files one hop from the start are visited in path order.
    let index = Tree::new()
        .imports("start.ts", "z.ts")
        .calls("start.ts", "http GET /x")
        .serves("a.ts", "http GET /x")
        .imports("a.ts", "goal.ts")
        .imports("z.ts", "goal.ts")
        .build();

    assert_eq!(
        steps(&route(&index, "start.ts", "goal.ts")),
        [
            ("start.ts", None),
            ("a.ts", Some("by http GET /x".to_owned())),
            ("goal.ts", Some("by import".to_owned())),
        ]
    );
}

#[test]
fn from_one_file_an_import_comes_before_a_contract() {
    let index = Tree::new()
        .imports("start.ts", "goal.ts")
        .calls("start.ts", "http GET /x")
        .serves("goal.ts", "http GET /x")
        .build();

    assert_eq!(
        steps(&route(&index, "start.ts", "goal.ts")),
        [
            ("start.ts", None),
            ("goal.ts", Some("by import".to_owned()))
        ]
    );
}

#[test]
fn from_one_file_contracts_come_in_key_order() {
    let index = Tree::new()
        .calls("start.ts", "queue beta")
        .calls("start.ts", "queue alpha")
        .serves("goal.ts", "queue beta")
        .serves("goal.ts", "queue alpha")
        .build();

    assert_eq!(
        steps(&route(&index, "start.ts", "goal.ts")),
        [
            ("start.ts", None),
            ("goal.ts", Some("by queue alpha".to_owned()))
        ]
    );
}

#[test]
fn a_file_that_calls_a_contract_after_a_server_reached_it_still_reaches_the_other_servers() {
    // `server.ts` crosses the contract first, as a server, and reaches only its declarers. `caller.ts` is one hop
    // farther, so it crosses the same contract again, as a caller, and reaches the other server.
    let index = Tree::new()
        .imports("start.ts", "server.ts")
        .serves("server.ts", "http GET /x")
        .imports("server.ts", "caller.ts")
        .calls("caller.ts", "http GET /x")
        .serves("other-server.ts", "http GET /x")
        .build();

    assert_eq!(
        steps(&route(&index, "start.ts", "other-server.ts")),
        [
            ("start.ts", None),
            ("server.ts", Some("by import".to_owned())),
            ("caller.ts", Some("by import".to_owned())),
            ("other-server.ts", Some("by http GET /x".to_owned())),
        ]
    );
}

#[test]
fn a_file_that_serves_and_calls_a_contract_starts_a_chain_as_a_caller() {
    let index = Tree::new()
        .site("proxy.ts", "http GET /x", Role::Serves, 3)
        .site("proxy.ts", "http GET /x", Role::Calls, 9)
        .serves("server.ts", "http GET /x")
        .declares("schema.yaml", "http GET /x")
        .build();

    assert_eq!(
        files(&route(&index, "proxy.ts", "server.ts")),
        ["proxy.ts", "server.ts"]
    );
    assert_eq!(
        files(&route(&index, "proxy.ts", "schema.yaml")),
        ["proxy.ts", "schema.yaml"]
    );
    // From a server, the file's site as a server is no hop: only the schema is.
    assert_eq!(route(&index, "server.ts", "proxy.ts").reach, Some(1));
}

#[test]
fn a_file_is_at_zero_hops_from_itself() {
    let index = Tree::new().imports("a.ts", "b.ts").build();

    let answer = route(&index, "a.ts", "a.ts");

    assert_eq!(steps(&answer), [("a.ts", None)]);
    assert!(
        answer
            .to_string()
            .starts_with("path from a.ts to a.ts, 0 hops\na.ts\n")
    );
}

#[test]
fn a_mod_declaration_is_named_as_one() {
    let index = Tree::new()
        .import(
            "src/lib.rs",
            Import::new("store", ImportKind::Module, 1),
            file("src/store.rs"),
        )
        .build();

    let answer = route(&index, "src/lib.rs", "src/store.rs");

    assert_eq!(
        steps(&answer),
        [
            ("src/lib.rs", None),
            ("src/store.rs", Some("by mod declaration".to_owned()))
        ]
    );
    assert_eq!(
        answer.steps.get(1).and_then(|step| step.by.clone()),
        Some(Link::Import {
            kinds: vec![ImportKind::Module],
            structural: true
        })
    );
}

#[test]
fn a_use_beside_the_mod_declaration_is_named_an_import() {
    let index = Tree::new()
        .import(
            "src/lib.rs",
            Import::new("store", ImportKind::Module, 1),
            file("src/store.rs"),
        )
        .import(
            "src/lib.rs",
            Import::new("crate::store::Store", ImportKind::Static, 2),
            file("src/store.rs"),
        )
        .build();

    let answer = route(&index, "src/lib.rs", "src/store.rs");

    assert_eq!(
        steps(&answer),
        [
            ("src/lib.rs", None),
            ("src/store.rs", Some("by import".to_owned()))
        ]
    );
    assert_eq!(
        answer.steps.get(1).and_then(|step| step.by.clone()),
        Some(Link::Import {
            kinds: vec![ImportKind::Static, ImportKind::Module],
            structural: false
        })
    );
}

#[test]
fn a_caller_reaches_the_server_and_the_schema_but_not_the_other_way() {
    let index = Tree::new()
        .read_by("openapi", "openapi.yaml")
        .declares("openapi.yaml", "http GET /bookings/{}")
        .serves("server.ts", "http GET /bookings/{}")
        .calls("client.ts", "http GET /bookings/{}")
        .build();

    assert_eq!(
        files(&route(&index, "client.ts", "server.ts")),
        ["client.ts", "server.ts"]
    );
    assert_eq!(
        files(&route(&index, "client.ts", "openapi.yaml")),
        ["client.ts", "openapi.yaml"]
    );
    assert_eq!(
        files(&route(&index, "server.ts", "openapi.yaml")),
        ["server.ts", "openapi.yaml"]
    );
    assert_eq!(route(&index, "server.ts", "client.ts").reach, Some(1));
    assert_eq!(route(&index, "openapi.yaml", "server.ts").reach, Some(0));
}

#[test]
fn a_hop_across_http_follows_keys_that_meet() {
    let index = Tree::new()
        .calls("app.ts", "http GET /bookings/me")
        .serves("controller.ts", "http GET /bookings/{}")
        .build();

    let answer = route(&index, "app.ts", "controller.ts");

    assert_eq!(
        steps(&answer),
        [
            ("app.ts", None),
            ("controller.ts", Some("by http GET /bookings/{}".to_owned()))
        ]
    );
}

#[test]
fn a_path_the_index_does_not_hold_is_named() {
    let index = Tree::new()
        .imports("a.ts", "b.ts")
        .unread("notes.txt")
        .build();

    let answer = route(&index, "a.ts", "notes.txt");

    assert_eq!(answer.unknown, ["notes.txt"]);
    assert_eq!(answer.reach, None);
    assert!(
        answer
            .to_string()
            .starts_with("notes.txt is not in the index\n")
    );
}

/// A tree of `size` files named by their number, with an import from `from` to `to` for each pair given.
fn numbered(size: usize, imports: &[(usize, usize)]) -> (Index, Vec<String>) {
    let names: Vec<String> = (0..size).map(|n| format!("f{n:02}.ts")).collect();
    let mut tree = Tree::new();
    for name in &names {
        tree = tree.tag(name, FileTags::NONE);
    }
    for &(from, to) in imports {
        tree = tree.imports(&names[from], &names[to]);
    }
    (tree.build(), names)
}

/// The number of hops from `from` to every file, by a plain breadth-first search over the pairs.
fn oracle(size: usize, imports: &[(usize, usize)], from: usize) -> Vec<Option<usize>> {
    let mut distance = vec![None; size];
    distance[from] = Some(0);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        for &(a, b) in imports {
            if a == at && distance[b].is_none() {
                distance[b] = distance[at].map(|hops| hops + 1);
                queue.push_back(b);
            }
        }
    }
    distance
}

proptest! {
    #[test]
    fn a_chain_has_the_length_of_the_shortest_one_and_no_chain_the_reach_of_the_search(
        size in 2usize..8,
        pairs in proptest::collection::vec((0usize..8, 0usize..8), 0..24),
        from in 0usize..8,
        to in 0usize..8,
    ) {
        let imports: Vec<(usize, usize)> = pairs
            .into_iter()
            .map(|(a, b)| (a % size, b % size))
            .filter(|(a, b)| a != b)
            .collect();
        let (from, to) = (from % size, to % size);
        let (index, names) = numbered(size, &imports);
        let wanted = oracle(size, &imports, from);

        let answer = route(&index, &names[from], &names[to]);

        if let Some(hops) = wanted[to] {
            prop_assert_eq!(answer.steps.len(), hops + 1);
            prop_assert_eq!(answer.reach, None);
            prop_assert_eq!(answer.steps.first().map(|s| s.path.as_str()), Some(names[from].as_str()));
            prop_assert_eq!(answer.steps.last().map(|s| s.path.as_str()), Some(names[to].as_str()));
            // Every step of the chain is an import that exists.
            for pair in answer.steps.windows(2) {
                let from_name = &pair[0].path;
                let to_name = &pair[1].path;
                let a = names.iter().position(|n| n == from_name).unwrap();
                let b = names.iter().position(|n| n == to_name).unwrap();
                prop_assert!(imports.contains(&(a, b)), "{from_name} does not import {to_name}");
            }
        } else {
            prop_assert!(answer.steps.is_empty());
            let reached = wanted.iter().flatten().count() - 1;
            prop_assert_eq!(answer.reach, Some(u32::try_from(reached).unwrap()));
        }
    }
}
