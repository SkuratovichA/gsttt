//! `impact` and `path` over GraphQL contracts, which cross only within an endpoint when the index has endpoints.

use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{ContractLookup, Found, ImpactRequest, contract, impact};
use gsttt_core::testing::Tree;
use gsttt_scenario::scenario;

use super::impact::{request, rows, run};
use super::paths::{files, route};

const USER: &str = "graphql Query.user";

/// Two endpoints, `a` and `b`: the clients of each are under `web-<name>/` and its servers under `api-<name>/`.
fn two_endpoints() -> Tree {
    Tree::new()
        .endpoint(&["web-a/**"], &["api-a/**"])
        .endpoint(&["web-b/**"], &["api-b/**"])
}

/// A server and a page of each endpoint resolve and query `key`.
fn users(tree: Tree, key: &str) -> Tree {
    tree.serves("api-a/users.ts", key)
        .serves("api-b/users.ts", key)
        .calls("web-a/page.ts", key)
        .calls("web-b/page.ts", key)
}

/// One more row of the answer: the path, the distance and the reason of an entry, as `rows` gives them.
fn row(path: &str, distance: u32, reason: &str) -> (String, u32, String) {
    (path.to_owned(), distance, reason.to_owned())
}

/// Asserts that no chain joins `from` to `to`.
fn no_chain(index: &Index, from: &str, to: &str) {
    let answer = route(index, from, to);
    assert_eq!(files(&answer), Vec::<&str>::new(), "{answer}");
}

/// `rows` of an answer with its paths owned, to compare with `row`.
fn seen(answer: &gsttt_core::query::Impact) -> Vec<(String, u32, String)> {
    rows(&answer.affected)
        .into_iter()
        .map(|(path, distance, reason)| (path.to_owned(), distance, reason))
        .collect()
}

#[scenario("queries", "A change reaches the callers of its own endpoint only")]
#[test]
fn a_change_reaches_the_callers_of_its_own_endpoint_only() {
    let index = users(two_endpoints(), USER).build();

    let answer = run(&index, &["api-a/users.ts"]);

    assert_eq!(
        seen(&answer),
        [row("web-a/page.ts", 1, "calls graphql Query.user")]
    );
}

#[scenario("queries", "A chain runs within an endpoint only")]
#[test]
fn a_chain_runs_within_an_endpoint_only() {
    let index = users(two_endpoints(), USER).build();

    assert_eq!(
        files(&route(&index, "web-a/page.ts", "api-a/users.ts")),
        ["web-a/page.ts", "api-a/users.ts"]
    );
    let across = route(&index, "web-a/page.ts", "api-b/users.ts");
    assert!(across.steps.is_empty(), "{across}");
    // The server of its own endpoint is the one file the page reaches.
    assert_eq!(across.reach, Some(1));
}

#[scenario("queries", "A schema reaches the sites of its own endpoint only")]
#[test]
fn a_schema_reaches_the_sites_of_its_own_endpoint_only() {
    let index = users(two_endpoints(), USER)
        .declares("api-a/schema.graphql", USER)
        .declares("api-b/schema.graphql", USER)
        .build();

    let answer = run(&index, &["api-a/schema.graphql"]);

    assert_eq!(
        seen(&answer),
        [
            row("api-a/users.ts", 1, "serves graphql Query.user"),
            row("web-a/page.ts", 1, "calls graphql Query.user"),
        ]
    );
    assert_eq!(
        files(&route(&index, "web-b/page.ts", "api-b/schema.graphql")),
        ["web-b/page.ts", "api-b/schema.graphql"]
    );
    no_chain(&index, "web-b/page.ts", "api-a/schema.graphql");
}

#[scenario("queries", "A site in no endpoint meets by the role it has")]
#[test]
fn a_site_in_no_endpoint_meets_by_the_role_it_has() {
    let index = users(two_endpoints(), USER)
        .calls("scripts/probe.ts", USER)
        .serves("legacy/users.ts", USER)
        .build();

    // A caller in no endpoint reaches the servers of every endpoint, and they reach it.
    assert_eq!(
        seen(&run(&index, &["api-a/users.ts"])),
        [
            row("scripts/probe.ts", 1, "calls graphql Query.user"),
            row("web-a/page.ts", 1, "calls graphql Query.user"),
        ]
    );
    for server in ["api-a/users.ts", "api-b/users.ts", "legacy/users.ts"] {
        assert_eq!(
            files(&route(&index, "scripts/probe.ts", server)),
            ["scripts/probe.ts", server]
        );
    }
    // A server in no endpoint is reached by callers in no endpoint only, because the endpoint of a page does not include it.
    assert_eq!(
        seen(&run(&index, &["legacy/users.ts"])),
        [row("scripts/probe.ts", 1, "calls graphql Query.user")]
    );
    no_chain(&index, "web-a/page.ts", "legacy/users.ts");
}

/// `web/` calls the gateway, which serves `a` from `gateway/` and `schema-a/` and calls `b`, served from `api/`. The gateway queries and resolves the same field.
fn gateway() -> Tree {
    Tree::new()
        .endpoint(&["web/**"], &["gateway/**", "schema-a/**"])
        .endpoint(&["gateway/**"], &["api/**"])
        .imports("entry.ts", "gateway/graph.ts")
        .serves("gateway/graph.ts", USER)
        .calls("gateway/graph.ts", USER)
        .declares("schema-a/user.graphql", USER)
        .serves("api/users.ts", USER)
        .calls("web/page.ts", USER)
}

#[scenario(
    "queries",
    "A file that serves one endpoint and calls another crosses both"
)]
#[test]
fn a_file_that_serves_one_endpoint_and_calls_another_crosses_both() {
    let index = gateway().build();

    // The gateway reaches the schema of `a` as its server, and the server of `b` as its caller.
    assert_eq!(
        files(&route(&index, "entry.ts", "schema-a/user.graphql")),
        ["entry.ts", "gateway/graph.ts", "schema-a/user.graphql"]
    );
    assert_eq!(
        files(&route(&index, "entry.ts", "api/users.ts")),
        ["entry.ts", "gateway/graph.ts", "api/users.ts"]
    );
    // A page of `a` is no client of `b`, so it reaches the server of `b` through the gateway only.
    assert_eq!(
        files(&route(&index, "web/page.ts", "api/users.ts")),
        ["web/page.ts", "gateway/graph.ts", "api/users.ts"]
    );
    // And the change of that server reaches the page the same way, back.
    assert_eq!(
        seen(&run(&index, &["api/users.ts"])),
        [
            row("gateway/graph.ts", 1, "calls graphql Query.user"),
            row("entry.ts", 2, "imports gateway/graph.ts"),
            row("web/page.ts", 2, "calls graphql Query.user"),
        ]
    );
}

#[scenario("queries", "Without endpoints a GraphQL contract meets every site")]
#[test]
fn without_endpoints_a_graphql_contract_meets_every_site() {
    let index = users(Tree::new(), USER).build();

    assert_eq!(
        seen(&run(&index, &["api-a/users.ts"])),
        [
            row("web-a/page.ts", 1, "calls graphql Query.user"),
            row("web-b/page.ts", 1, "calls graphql Query.user"),
        ]
    );
    assert_eq!(
        files(&route(&index, "web-a/page.ts", "api-b/users.ts")),
        ["web-a/page.ts", "api-b/users.ts"]
    );
}

#[scenario("queries", "Endpoints do not scope other contracts")]
#[test]
fn endpoints_do_not_scope_other_contracts() {
    let index = users(two_endpoints(), "http GET /user").build();

    assert_eq!(
        seen(&run(&index, &["api-a/users.ts"])),
        [
            row("web-a/page.ts", 1, "calls http GET /user"),
            row("web-b/page.ts", 1, "calls http GET /user"),
        ]
    );
    assert_eq!(
        files(&route(&index, "web-a/page.ts", "api-b/users.ts")),
        ["web-a/page.ts", "api-b/users.ts"]
    );
}

#[test]
fn files_of_different_endpoints_in_one_layer_each_reach_their_own_callers() {
    let index = users(two_endpoints(), USER).build();

    let answer = run(&index, &["api-a/users.ts", "api-b/users.ts"]);

    assert_eq!(
        seen(&answer),
        [
            row("web-a/page.ts", 1, "calls graphql Query.user"),
            row("web-b/page.ts", 1, "calls graphql Query.user"),
        ]
    );
}

#[test]
fn files_of_different_endpoints_in_one_layer_of_a_chain_each_reach_their_own_servers() {
    let index = users(two_endpoints(), USER)
        .imports("start.ts", "web-a/page.ts")
        .imports("start.ts", "web-b/page.ts")
        .build();

    assert_eq!(
        files(&route(&index, "start.ts", "api-b/users.ts")),
        ["start.ts", "web-b/page.ts", "api-b/users.ts"]
    );
    assert_eq!(
        files(&route(&index, "start.ts", "api-a/users.ts")),
        ["start.ts", "web-a/page.ts", "api-a/users.ts"]
    );
}

#[test]
fn a_site_reached_only_by_the_farther_of_two_leavers_has_the_hops_of_that_leaver() {
    // `api-a/users.ts` is reached by an import, with one hop through code, and `api-b/users.ts` across a contract, with none. Each is a leaver of `Query.user` for its own endpoint, so the page of `a` is one hop through code from the change and not none, though the server of `b` is nearer.
    let index = two_endpoints()
        .serves("lib.ts", "http GET /seed")
        .imports("api-a/users.ts", "lib.ts")
        .serves("api-a/users.ts", USER)
        .calls("api-b/users.ts", "http GET /seed")
        .serves("api-b/users.ts", USER)
        .calls("web-a/page.ts", USER)
        .imports("web-a/entry.ts", "web-a/page.ts")
        .calls("web-b/page.ts", USER)
        .imports("web-b/entry.ts", "web-b/page.ts")
        .build();
    let request = ImpactRequest {
        depth: Some(1),
        ..request(&["lib.ts"])
    };

    let answer = impact(&index, Freshness::Fresh, &request);

    assert_eq!(
        seen(&answer),
        [
            row("api-a/users.ts", 1, "imports lib.ts"),
            row("api-b/users.ts", 1, "calls http GET /seed"),
            row("web-a/page.ts", 2, "calls graphql Query.user"),
            row("web-b/page.ts", 2, "calls graphql Query.user"),
            row("web-b/entry.ts", 3, "imports web-b/page.ts"),
        ]
    );
}

#[test]
fn the_contract_query_lists_every_site_whatever_the_endpoints() {
    let tree = users(two_endpoints(), USER);

    let answer: ContractLookup = contract(&tree.build(), Freshness::Fresh, USER).unwrap();

    let Found::Key { serves, calls, .. } = answer.found else {
        panic!("expected the sites of a key");
    };
    let paths = |sites: &[gsttt_core::query::Site]| {
        sites
            .iter()
            .map(|site| site.path.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&serves), ["api-a/users.ts", "api-b/users.ts"]);
    assert_eq!(paths(&calls), ["web-a/page.ts", "web-b/page.ts"]);
}
