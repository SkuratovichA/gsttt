//! A call on a client or a router that another file makes, read on what that file makes under the name.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, run_with_pack, sites, unread};

#[test]
#[scenario("typescript", "A client made in another file is called")]
fn a_client_made_in_another_file_is_called() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-client")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /api/users @3 in src/users.ts"]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A router made in another file serves under its mounts")]
fn a_router_made_in_another_file_serves_under_its_mounts() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-router")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /api/users @3 in src/users.ts"]
    );
    assert_eq!(
        sites("receivers-router")?,
        ["serves http GET /api/users @3 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A call on an imported name that no rule makes is silent"
)]
fn a_call_on_an_imported_name_that_no_rule_makes_is_silent() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-silent")?;
    // The call on the client is read, which only a link that finds what another file makes can do, so this test fails when imported receivers are not read at all; the two calls on the map are not.
    let client = ["calls http GET /b/users @7 in src/a.ts"];
    assert_eq!(linked(&json), client);
    assert_eq!(sites("receivers-silent")?, client);
    assert_eq!(contracts(&json, "src/a.ts"), NOTHING);
    assert_eq!(unread(&json, "src/a.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A call on the default import of a file that makes a client is the client's"
)]
fn a_call_on_the_default_import_of_a_file_that_makes_a_client_is_the_clients()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-default-client")?;
    assert_eq!(
        sites("receivers-default-client")?,
        [
            "calls http GET /api/users @3 in src/users.ts",
            "calls http GET /orders @3 in src/orders.ts",
        ]
    );
    // The parse reads neither call: the link decides which rule reads each.
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(contracts(&json, "src/orders.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/orders.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn an_imported_constant_url_on_an_imported_client_is_read_once_under_its_base()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-imported-url")?;
    assert_eq!(
        sites("receivers-imported-url")?,
        ["calls http GET /api/users @4 in src/users.ts"]
    );
    assert_eq!(
        linked(&json),
        ["calls http GET /api/users @4 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_base_url_imported_into_the_file_that_makes_the_client_is_its_prefix()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-base-url")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /v2/users @3 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_router_exported_by_default_without_a_name_is_served_under_its_mounts()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-default-router")?;
    assert_eq!(linked(&json), ["serves http GET /api/x @3 in src/users.ts"]);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_call_on_a_package_or_node_stays_a_fact_of_the_parse() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-library")?;
    assert_eq!(contracts(&json, "src/client.ts"), ["calls http GET /x @4"]);
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(unread(&json, "src/client.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_member_of_an_imported_client_is_no_client() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-client-member")?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A client made inside a function does not replace the one a file exports"
)]
fn a_client_made_inside_a_function_does_not_replace_the_one_a_file_exports()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-nested-client")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /a/users @3 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_variable_another_call_makes_inside_a_function_leaves_the_export_in_place()
-> Result<(), Box<dyn Error>> {
    // The functions come before and after the export, so the export wins whichever the walk meets first.
    let json = run("receivers-nested-load")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /a/users @3 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_client_made_inside_a_function_is_no_export_of_the_file() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-nested-only")?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn an_imported_router_that_nothing_mounts_serves_the_paths_it_is_written_with()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-router-unmounted")?;
    assert_eq!(linked(&json), ["serves http GET /users @3 in src/users.ts"]);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A call on an imported name is read by the rule of the call that made it"
)]
fn a_call_on_an_imported_name_is_read_by_the_rule_of_the_call_that_made_it()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-by-maker")?;
    // `app.get('env')` and `app.get('/k')` on the Express app are no client call, and `api.get('/users', { params })` on the client is no route.
    let client = ["calls http GET /b/users @7 in src/a.ts"];
    assert_eq!(linked(&json), client);
    assert_eq!(sites("receivers-by-maker")?, client);
    assert_eq!(contracts(&json, "src/a.ts"), NOTHING);
    assert_eq!(unread(&json, "src/a.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_client_re_exported_by_a_barrel_is_read_under_its_base() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-barrel")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /b/users @3 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_client_taken_through_a_namespace_import_is_read_under_its_base() -> Result<(), Box<dyn Error>>
{
    let json = run("receivers-namespace")?;
    assert_eq!(linked(&json), ["calls http GET /b/x @3 in src/users.ts"]);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_client_exported_by_default_through_its_name_is_read_under_its_base()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-default-local")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /b/users @3 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
fn an_unreadable_url_on_an_imported_client_is_one_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("receivers-unread-url")?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts").len(), 1);
    assert!(unread(&json, "src/users.ts")[0].starts_with("@4 "));
    Ok(())
}

#[test]
fn a_computed_url_on_an_imported_client_with_no_base_is_one_unread_site()
-> Result<(), Box<dyn Error>> {
    let json = run("receivers-no-base")?;
    assert_eq!(linked(&json), ["calls http GET /users @7 in src/a.ts"]);
    assert_eq!(contracts(&json, "src/a.ts"), NOTHING);
    assert_eq!(unread(&json, "src/a.ts"), ["@6 non-literal axios.get"]);
    Ok(())
}

/// A pack that serves `get` on a router made by `makeRouter`, behind the prefix the `prefix` property of the maker's argument holds.
const ROUTER_PACK: &str = r#"
name = "fixture"
language = "ts"

[[calls]]
callee = "*.get"
kind = "http"
role = "serves"
key = "$arg1"
method = "$callee"
receiver = { from = "makeRouter", key = "$arg1.prefix" }
"#;

#[test]
#[scenario(
    "typescript",
    "A route on a router made in the same file with a prefix that is not text is unread"
)]
fn a_route_on_a_router_made_here_with_a_prefix_that_is_not_text_is_unread()
-> Result<(), Box<dyn Error>> {
    let json = run_with_pack("receivers-unread-prefix", ROUTER_PACK)?;
    assert_eq!(
        unread(&json, "src/router.ts"),
        ["@9 non-literal dynamic.get"]
    );
    let served: Vec<String> = linked(&json)
        .into_iter()
        .filter(|fact| fact.ends_with("in src/router.ts"))
        .collect();
    assert_eq!(
        served,
        [
            "serves http GET /own @11 in src/router.ts",
            "serves http GET /v1/own @10 in src/router.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A route on a router made in another file with a prefix that is not text is unread"
)]
fn a_route_on_a_router_made_in_another_file_with_a_prefix_that_is_not_text_is_unread()
-> Result<(), Box<dyn Error>> {
    let json = run_with_pack("receivers-unread-prefix", ROUTER_PACK)?;
    assert_eq!(
        unread(&json, "src/users.ts"),
        ["@5 non-literal dynamic.get"]
    );
    let served: Vec<String> = linked(&json)
        .into_iter()
        .filter(|fact| fact.ends_with("in src/users.ts"))
        .collect();
    assert_eq!(
        served,
        [
            "serves http GET /v1/orders @6 in src/users.ts",
            "serves http GET /items @7 in src/users.ts",
        ]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A request on a client made in another file is called")]
fn a_request_on_a_client_made_in_another_file_is_called() -> Result<(), Box<dyn Error>> {
    let json = run("axios-request-imported")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /api/users @3 in src/users.ts"]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}
