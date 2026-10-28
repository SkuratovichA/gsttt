//! HTTP routes and requests.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, unread};

#[test]
#[scenario("typescript", "An Express route is served")]
fn an_express_route_is_served() -> Result<(), Box<dyn Error>> {
    let json = run("express-route")?;
    assert_eq!(linked(&json), ["serves http GET /health @4 in src/app.ts"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A call on another receiver is no route")]
fn a_call_on_another_receiver_is_no_route() -> Result<(), Box<dyn Error>> {
    let json = run("express-other-receiver")?;
    assert_eq!(contracts(&json, "src/cache.ts"), NOTHING);
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(unread(&json, "src/cache.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A router mounted in another file serves under the mount"
)]
fn a_router_mounted_in_another_file_serves_under_the_mount() -> Result<(), Box<dyn Error>> {
    let json = run("express-mounts")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /api/bookings/{} @4 in src/bookings.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A NestJS controller serves its routes")]
fn a_nestjs_controller_serves_its_routes() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-controller")?;
    assert_eq!(
        contracts(&json, "src/bookings.controller.ts"),
        ["serves http GET /bookings/{} @5"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Requests become calls")]
fn requests_become_calls() -> Result<(), Box<dyn Error>> {
    let json = run("requests")?;
    assert_eq!(
        contracts(&json, "src/client.ts"),
        [
            "calls http DELETE /bookings/{} @4",
            "calls http POST /bookings @5"
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A base URL in a template is dropped")]
fn a_base_url_in_a_template_is_dropped() -> Result<(), Box<dyn Error>> {
    let json = run("template-base-url")?;
    assert_eq!(
        contracts(&json, "src/api.ts"),
        ["calls http * /bookings @3"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A computed URL is a blind spot")]
fn a_computed_url_is_a_blind_spot() -> Result<(), Box<dyn Error>> {
    let json = run("computed-url")?;
    assert_eq!(unread(&json, "src/api.ts"), ["@1 non-literal fetch"]);
    assert_eq!(contracts(&json, "src/api.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}
