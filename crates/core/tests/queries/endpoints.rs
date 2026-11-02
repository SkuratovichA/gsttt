//! `impact` and `path` over GraphQL contracts, which cross only within an endpoint when the index has endpoints.

use gsttt_core::contract::Role;
use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{ContractLookup, Found, ImpactRequest, contract, impact};
use gsttt_core::testing::{Code, Tree};
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

#[scenario(
    "queries",
    "An endpoint served outside the repository keeps its callers apart"
)]
#[test]
fn an_endpoint_served_outside_the_repository_keeps_its_callers_apart() {
    let tree = || {
        Tree::new()
            .calls("web-a/page.ts", USER)
            .serves("api/users.ts", USER)
    };
    // Without the endpoint the page and the server meet.
    assert_eq!(
        seen(&run(&tree().build(), &["api/users.ts"])),
        [row("web-a/page.ts", 1, "calls graphql Query.user")]
    );

    let index = tree().endpoint(&["web-a/**"], &[]).build();

    assert_eq!(seen(&run(&index, &["api/users.ts"])), Vec::new());
    no_chain(&index, "web-a/page.ts", "api/users.ts");
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

#[scenario(
    "queries",
    "The contract query lists every site whatever the endpoints"
)]
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

/// Asks about the `names` declared in `file`, with the depth of the request left as given.
fn ask_names(
    index: &Index,
    file: &str,
    names: &[&str],
    depth: Option<u32>,
) -> gsttt_core::query::Impact {
    let request = ImpactRequest {
        files: vec![file.to_owned()],
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        depth,
        ..ImpactRequest::default()
    };
    impact(index, Freshness::Fresh, &request)
}

#[scenario(
    "queries",
    "A walk by name crosses a GraphQL contract within an endpoint"
)]
#[test]
fn a_walk_by_name_crosses_a_graphql_contract_within_an_endpoint() {
    // `lib/user.ts` is in no endpoint, so the scope of the site the walk leaves by is that of the file the walk reached, `api-a/users.ts`, and not that of the file the name is declared in.
    let index = users(two_endpoints(), USER)
        .symbols(
            "lib/user.ts",
            Code::new().declaration("loadUser", (1, 5), &[]),
        )
        .exports("lib/user.ts", &["loadUser"])
        .imports_names("api-a/users.ts", "lib/user.ts", &["loadUser"])
        .symbols(
            "api-a/users.ts",
            Code::new().declaration("resolveUser", (1, 5), &["loadUser"]),
        )
        .exports("api-a/users.ts", &["resolveUser"])
        .build();

    let answer = ask_names(&index, "lib/user.ts", &["loadUser"], None);

    assert_eq!(
        seen(&answer),
        [
            row("api-a/users.ts", 1, "imports loadUser from lib/user.ts"),
            row("web-a/page.ts", 2, "calls graphql Query.user"),
        ]
    );
}

/// `api-<near>/lib.ts` declares `foo` and serves `Query.other`, which `api-<near>/users.ts` calls, so its server of `Query.user` is reached across a contract, with no hop through code. `api-<far>/users.ts` uses `foo`, so its server of `Query.user` is reached with one. Each page has an entry that imports it.
fn two_servers_at_different_hops(near: &str, far: &str) -> Index {
    let mut tree = users(two_endpoints(), USER)
        .site(
            &format!("api-{near}/lib.ts"),
            "graphql Query.other",
            Role::Serves,
            2,
        )
        .site(
            &format!("api-{near}/users.ts"),
            "graphql Query.other",
            Role::Calls,
            2,
        )
        .symbols(
            &format!("api-{near}/lib.ts"),
            Code::new().declaration("foo", (1, 5), &[]),
        )
        .exports(&format!("api-{near}/lib.ts"), &["foo"])
        .imports_names(
            &format!("api-{far}/users.ts"),
            &format!("api-{near}/lib.ts"),
            &["foo"],
        )
        .symbols(
            &format!("api-{far}/users.ts"),
            Code::new().declaration("resolve", (1, 5), &["foo"]),
        )
        .exports(&format!("api-{far}/users.ts"), &["resolve"])
        .symbols(
            &format!("api-{near}/users.ts"),
            Code::new().declaration("resolve", (1, 5), &[]),
        )
        .exports(&format!("api-{near}/users.ts"), &["resolve"]);
    for side in [near, far] {
        let page = format!("web-{side}/page.ts");
        let entry = format!("web-{side}/entry.ts");
        tree = tree
            .symbols(&page, Code::new().declaration("show", (1, 5), &[]))
            .exports(&page, &["show"])
            .imports_names(&entry, &page, &["show"])
            .symbols(&entry, Code::new().declaration("main", (1, 5), &["show"]))
            .exports(&entry, &[]);
    }
    tree.build()
}

#[test]
fn the_hops_through_code_across_a_contract_are_those_of_the_servers_of_its_endpoint() {
    // Each page is as far in hops through code as the server of its own endpoint, whichever endpoint that is, so with a depth of one the file that imports the page of the near endpoint is reached and the one that imports the page of the far endpoint is not.
    for (near, far) in [("b", "a"), ("a", "b")] {
        let index = two_servers_at_different_hops(near, far);

        let answer = ask_names(&index, &format!("api-{near}/lib.ts"), &["foo"], Some(1));

        let mut expected = vec![
            row(
                &format!("api-{far}/users.ts"),
                1,
                &format!("imports foo from api-{near}/lib.ts"),
            ),
            row(
                &format!("api-{near}/users.ts"),
                1,
                "calls graphql Query.other",
            ),
            row("web-a/page.ts", 2, "calls graphql Query.user"),
            row("web-b/page.ts", 2, "calls graphql Query.user"),
            row(
                &format!("web-{near}/entry.ts"),
                3,
                &format!("imports show from web-{near}/page.ts"),
            ),
        ];
        expected.sort_by(|left, right| (left.1, &left.0).cmp(&(right.1, &right.0)));
        assert_eq!(seen(&answer), expected, "near endpoint {near}");
    }
}
