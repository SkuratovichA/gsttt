//! A link that reads the packages of a monorepo its project imports: constants, routers and controllers of another package.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, run, sites, unread};

#[test]
#[scenario("typescript", "A URL reads a constant of another package")]
fn a_url_reads_a_constant_of_another_package() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-url-constants")?;
    assert_eq!(
        sites("workspace-url-constants")?,
        [
            "calls http GET /api/v1/users @5 in packages/web/src/client.ts",
            "calls http GET /api/v1/users/list @4 in packages/web/src/client.ts",
        ]
    );
    assert_eq!(unread(&json, "packages/web/src/client.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mount reads a constant of another package")]
fn a_mount_reads_a_constant_of_another_package() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-mount-constant")?;
    assert_eq!(
        sites("workspace-mount-constant")?,
        ["serves http GET /api/v1/list @4 in packages/api/src/users.ts",]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A router of another package mounted by an app serves under the mount"
)]
fn a_router_of_another_package_mounted_by_an_app_serves_under_the_mount()
-> Result<(), Box<dyn Error>> {
    let json = run("workspace-router")?;
    assert_eq!(
        sites("workspace-router")?,
        ["serves http GET /ext/x @4 in packages/routes/src/index.ts",]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A router of another package with a router of its own below it chains the prefixes"
)]
fn a_router_of_another_package_with_a_router_below_it_chains_the_prefixes()
-> Result<(), Box<dyn Error>> {
    let json = run("workspace-router-chain")?;
    assert_eq!(
        sites("workspace-router-chain")?,
        ["serves http GET /ext/inner/x @4 in packages/routes/src/inner.ts",]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Two apps mounting one router give both paths")]
fn two_apps_mounting_one_router_give_both_paths() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-router-two-apps")?;
    assert_eq!(
        sites("workspace-router-two-apps")?,
        [
            "serves http GET /admin/x @4 in packages/routes/src/index.ts",
            "serves http GET /api/x @4 in packages/routes/src/index.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A router of another package that no app mounts keeps its path"
)]
fn a_router_of_another_package_that_no_app_mounts_keeps_its_path() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-router-unmounted")?;
    assert_eq!(
        sites("workspace-router-unmounted")?,
        [
            "serves http GET /own @5 in packages/api/src/app.ts",
            "serves http GET /x @4 in packages/routes/src/index.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A controller of another package serves behind the global prefix of the app that reaches it"
)]
fn a_controller_of_another_package_serves_behind_the_global_prefix() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-controller")?;
    assert_eq!(
        sites("workspace-controller")?,
        [
            "serves http GET /api/items/{} @5 in packages/items/src/items.controller.ts",
            "serves http GET /legacy/{} @5 in packages/items/src/legacy.controller.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An outside package imported by name adds no site and no blind spot"
)]
fn an_outside_package_imported_by_name_adds_no_site_and_no_blind_spot() -> Result<(), Box<dyn Error>>
{
    let json = run("workspace-outside-package")?;
    assert_eq!(
        sites("workspace-outside-package")?,
        ["serves http GET /own @6 in packages/api/src/app.ts",]
    );
    assert_eq!(unread(&json, "packages/api/src/app.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "Routers imported by name under a mount each serve under it"
)]
fn routers_imported_by_name_under_a_mount_each_serve_under_it() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-mount-routers")?;
    assert_eq!(
        sites("workspace-mount-routers")?,
        [
            "serves http GET /w/ra @4 in packages/api/src/rs.ts",
            "serves http GET /w/rb @7 in packages/api/src/rs.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A prefix that reads through another package leaves no unread site"
)]
fn a_prefix_that_reads_through_another_package_leaves_no_unread_site() -> Result<(), Box<dyn Error>>
{
    let json = run("workspace-mount-template")?;
    assert_eq!(
        sites("workspace-mount-template")?,
        ["serves http GET /api/v1/two/e @4 in packages/api/src/e.ts"]
    );
    assert_eq!(unread(&json, "packages/api/src/app.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A prefix whose name comes from an outside package stays an unread site"
)]
fn a_prefix_whose_name_comes_from_an_outside_package_stays_an_unread_site()
-> Result<(), Box<dyn Error>> {
    let json = run("workspace-mount-outside")?;
    assert_eq!(unread(&json, "packages/api/src/app.ts").len(), 1);
    insta::assert_json_snapshot!(json);
    Ok(())
}
