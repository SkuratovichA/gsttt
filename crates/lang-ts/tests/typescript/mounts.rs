//! A `.use` on a name imported from another file, which is a mount only when that name is a router or an app.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, linked, run, sites, unread};

#[test]
#[scenario(
    "typescript",
    "A router mounted on an imported app serves under the mount"
)]
fn a_router_mounted_on_an_imported_app_serves_under_the_mount() -> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-app")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /api/users @5 in src/router.ts"]
    );
    assert_eq!(unread(&json, "src/main.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A router mounted on an imported router composes with the mounts above it"
)]
fn a_router_mounted_on_an_imported_router_composes_with_the_mounts_above_it()
-> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-router")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /api/sub/x @5 in src/sub.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A use on an imported name that is no router is silent")]
fn a_use_on_an_imported_name_that_is_no_router_is_silent() -> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-silent")?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(sites("imported-mount-silent")?, NOTHING);
    assert_eq!(unread(&json, "src/setup.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A use on an import from a package is silent")]
fn a_use_on_an_import_from_a_package_is_silent() -> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-package")?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(unread(&json, "src/setup.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A computed prefix on an imported app is an unread site")]
fn a_computed_prefix_on_an_imported_app_is_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-computed")?;
    assert_eq!(
        unread(&json, "src/main.ts"),
        ["@4 non-literal app.use(`${process.env.BASE}/api`, router)"]
    );
    // The same call on an imported name that is no router reports nothing.
    assert_eq!(unread(&json, "src/setup.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A mount on an app imported through a re-export serves under the mount"
)]
fn a_mount_on_an_app_imported_through_a_re_export_serves_under_the_mount()
-> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-reexport")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /api/users @5 in src/router.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A use on an imported client that serves no routes is silent"
)]
fn a_use_on_an_imported_client_that_serves_no_routes_is_silent() -> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-client")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /users @5 in src/router.ts"]
    );
    assert_eq!(unread(&json, "src/setup.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A use on a member of an imported name is silent")]
fn a_use_on_a_member_of_an_imported_name_is_silent() -> Result<(), Box<dyn Error>> {
    let json = run("imported-mount-member")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /users @5 in src/router.ts"]
    );
    assert_eq!(unread(&json, "src/setup.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}
