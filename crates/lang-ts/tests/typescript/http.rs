//! HTTP routes and requests.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, sites, unread};

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

#[test]
#[scenario("typescript", "A router mounted under several paths serves under each")]
fn a_router_mounted_under_several_paths_serves_under_each() -> Result<(), Box<dyn Error>> {
    let json = run("express-mount-forms")?;
    let mut served = linked(&json);
    served.sort();
    assert_eq!(
        served,
        [
            "serves http GET /a/z @4 in src/pair.ts",
            "serves http GET /b/z @4 in src/pair.ts",
            "serves http GET /c/m @4 in src/mixed.ts",
            "serves http GET /health @4 in src/extras.ts",
            "serves http GET /v1/list @4 in src/users.ts",
            "serves http GET /v2/list @4 in src/users.ts",
        ]
    );
    let sites = unread(&json, "src/app.ts");
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert!(sites[0].starts_with("@14 non-literal"), "{sites:?}");
    Ok(())
}

#[test]
#[scenario("typescript", "A mount cycle is cut where it closes")]
fn a_mount_cycle_is_cut_where_it_closes() -> Result<(), Box<dyn Error>> {
    let json = run("express-mount-cycle")?;
    let mut served = linked(&json);
    served.sort();
    assert_eq!(
        served,
        [
            "serves http GET /api/b/y @6 in src/b.ts",
            "serves http GET /api/x @6 in src/a.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A router mounted at the root serves without a prefix")]
fn a_router_mounted_at_the_root_serves_without_a_prefix() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("express-root-mount")?,
        ["serves http GET /x @7 in src/app.ts"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A router mounted at the root and under a prefix serves under both"
)]
fn a_router_mounted_at_the_root_and_under_a_prefix_serves_under_both() -> Result<(), Box<dyn Error>>
{
    assert_eq!(
        sites("express-root-and-prefix-mount")?,
        [
            "serves http GET /api/x @8 in src/app.ts",
            "serves http GET /x @8 in src/app.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "Routes on one line are keyed apart")]
fn routes_on_one_line_are_keyed_apart() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("express-routes-on-one-line")?,
        [
            "serves http GET /api/a @7 in src/app.ts",
            "serves http POST /api/b @7 in src/app.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A setting getter is no route")]
fn a_setting_getter_is_no_route() -> Result<(), Box<dyn Error>> {
    let json = run("express-settings")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /ping @5 in src/settings.ts"]
    );
    assert_eq!(contracts(&json, "src/settings.ts"), Vec::<String>::new());
    Ok(())
}
