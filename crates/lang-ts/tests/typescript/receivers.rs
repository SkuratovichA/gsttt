//! A call on a client or a router that another file makes, read on what that file makes under the name.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, sites, unread};

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
    assert_eq!(contracts(&json, "src/a.ts"), NOTHING);
    assert_eq!(unread(&json, "src/a.ts"), NOTHING);
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(sites("receivers-silent")?, NOTHING);
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
