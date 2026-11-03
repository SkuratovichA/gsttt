//! A package required where it is used, read the way one bound to a name is.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, sites};

#[test]
#[scenario("typescript", "A member of a package required in place is called")]
fn a_member_of_a_package_required_in_place_is_called() -> Result<(), Box<dyn Error>> {
    let json = run("required-call")?;
    assert_eq!(contracts(&json, "src/c.js"), ["calls http GET /c @2"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A router required in place serves its routes")]
fn a_router_required_in_place_serves_its_routes() -> Result<(), Box<dyn Error>> {
    let json = run("required-router")?;
    assert_eq!(
        linked(&json),
        [
            "serves http GET /a @3 in src/a.js",
            "serves http POST /a @4 in src/a.js"
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An application required in place serves its routes")]
fn an_application_required_in_place_serves_its_routes() -> Result<(), Box<dyn Error>> {
    let json = run("required-app")?;
    assert_eq!(linked(&json), ["serves http GET /b @3 in src/b.js"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A member of a package bound to a name is called")]
fn a_member_of_a_package_bound_to_a_name_is_called() -> Result<(), Box<dyn Error>> {
    let json = run("required-bound")?;
    assert_eq!(contracts(&json, "src/d.js"), ["calls http GET /d @3"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member of a project module bound to a name is read as the export"
)]
fn a_member_of_a_project_module_bound_to_a_name_is_read_as_the_export() -> Result<(), Box<dyn Error>>
{
    let json = run("required-named")?;
    assert_eq!(
        linked(&json),
        ["calls http GET /named/users @3 in src/users.js"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A require in place that no pack reads gives no fact")]
fn a_require_in_place_that_no_pack_reads_gives_no_fact() -> Result<(), Box<dyn Error>> {
    let json = run("required-unread")?;
    assert_eq!(contracts(&json, "src/other.js"), NOTHING);
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(sites("required-unread")?, NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A computed member of a required package gives no fact")]
fn a_computed_member_of_a_required_package_gives_no_fact() -> Result<(), Box<dyn Error>> {
    let json = run("required-computed")?;
    assert_eq!(contracts(&json, "src/x.js"), NOTHING);
    assert_eq!(sites("required-computed")?, NOTHING);
    Ok(())
}
