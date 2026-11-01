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
#[scenario(
    "typescript",
    "A NestJS controller imported as a namespace serves its routes"
)]
fn a_nestjs_controller_imported_as_a_namespace_serves_its_routes() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-controller-namespace")?;
    assert_eq!(
        contracts(&json, "src/users.controller.ts"),
        ["serves http GET /users/{} @5", "serves http POST /users @8"]
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
#[scenario("typescript", "A URL built from a path and a base is called")]
fn a_url_built_from_a_path_and_a_base_is_called() -> Result<(), Box<dyn Error>> {
    let json = run("new-url")?;
    assert_eq!(
        contracts(&json, "src/client.ts"),
        [
            "calls http POST /build-runner/success @7",
            "calls env API_URL @11",
            "calls http * /users @11"
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A URL built from a template that starts with a base URL keeps its path"
)]
fn a_url_built_from_a_template_that_starts_with_a_base_url_keeps_its_path()
-> Result<(), Box<dyn Error>> {
    let json = run("new-url-template")?;
    assert_eq!(
        contracts(&json, "src/client.ts"),
        ["calls http * /users @1", "calls http * /orders @2"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A relative path joins the directory of a literal base")]
fn a_relative_path_joins_the_directory_of_a_literal_base() -> Result<(), Box<dyn Error>> {
    let json = run("new-url-joined")?;
    assert_eq!(
        contracts(&json, "src/client.ts"),
        ["calls http * /v1/users/{} @1", "calls http * /users @2"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A URL built from a computed path is an unread site")]
fn a_url_built_from_a_computed_path_is_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("new-url-computed")?;
    assert_eq!(unread(&json, "src/client.ts"), ["@1 non-literal fetch"]);
    assert_eq!(contracts(&json, "src/client.ts"), NOTHING);
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
#[scenario(
    "typescript",
    "A URL whose path is only computed segments is a blind spot"
)]
fn a_url_whose_path_is_only_computed_segments_is_a_blind_spot() -> Result<(), Box<dyn Error>> {
    let json = run("placeholder-url")?;
    assert_eq!(
        unread(&json, "src/api.ts"),
        ["@4 non-literal fetch", "@6 non-literal fetch"]
    );
    assert_eq!(
        contracts(&json, "src/api.ts"),
        ["calls http * /users/{} @8"]
    );
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
#[scenario("typescript", "A router required in place serves under the mount")]
fn a_router_required_in_place_serves_under_the_mount() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("express-mount-require")?,
        ["serves http GET /items/{} @4 in src/items.js"]
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
#[scenario("typescript", "A fact of another rule on the line of a route is kept")]
fn a_fact_of_another_rule_on_the_line_of_a_route_is_kept() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        contracts(&run("express-and-nestjs-on-one-line")?, "src/app.ts"),
        ["calls http * /c @4", "serves http GET /x/a @4"]
    );
    assert_eq!(
        sites("express-and-nestjs-on-one-line")?,
        [
            "calls http * /c @4 in src/app.ts",
            "serves http GET /b @4 in src/app.ts",
            "serves http GET /x/a @4 in src/app.ts",
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

#[test]
#[scenario("typescript", "A route whose path is computed is a blind spot")]
fn a_route_whose_path_is_computed_is_a_blind_spot() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-computed-route")?;
    assert_eq!(
        unread(&json, "src/auth.controller.ts"),
        ["@6 non-literal Get"]
    );
    assert_eq!(
        contracts(&json, "src/auth.controller.ts"),
        ["serves http GET / @9"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A controller serves the routes of the base class it extends"
)]
fn a_controller_serves_the_routes_of_the_base_class_it_extends() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("nestjs-inherited-routes")?,
        [
            "serves http GET /users/{} @4 in src/user.controller.base.ts",
            "serves http GET /users/{} @4 in src/user.controller.ts",
            "serves http POST /users @4 in src/user.controller.ts",
            "serves http POST /users @7 in src/user.controller.base.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A base class that no controller extends serves nothing")]
fn a_base_class_that_no_controller_extends_serves_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-unextended-base")?;
    assert_eq!(contracts(&json, "src/user.controller.base.ts"), NOTHING);
    assert_eq!(unread(&json, "src/user.controller.base.ts"), NOTHING);
    assert_eq!(
        sites("nestjs-unextended-base")?,
        [
            "serves http GET /orders @4 in src/orders.controller.base.ts",
            "serves http GET /orders @4 in src/orders.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A subclass without the controller decorator serves no route of its base"
)]
fn a_subclass_without_the_controller_decorator_serves_no_route_of_its_base()
-> Result<(), Box<dyn Error>> {
    let json = run("nestjs-undecorated-subclass")?;
    assert_eq!(contracts(&json, "src/user.controller.ts"), NOTHING);
    assert_eq!(
        sites("nestjs-undecorated-subclass")?,
        [
            "serves http GET /users/{} @4 in src/user.controller.base.ts",
            "serves http GET /users/{} @4 in src/user.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A computed path on a base class is an unread site")]
fn a_computed_path_on_a_base_class_is_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-computed-base-route")?;
    assert_eq!(
        unread(&json, "src/user.controller.base.ts"),
        ["@5 non-literal Get"]
    );
    assert_eq!(contracts(&json, "src/user.controller.base.ts"), NOTHING);
    assert_eq!(sites("nestjs-computed-base-route")?, NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "Two controllers that extend one base class serve its routes under their own prefixes"
)]
fn two_controllers_that_extend_one_base_class_serve_its_routes_under_their_own_prefixes()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("nestjs-two-subclasses")?,
        [
            "serves http GET /admin/users/{} @4 in src/admin.controller.ts",
            "serves http GET /admin/users/{} @4 in src/user.controller.base.ts",
            "serves http GET /users/{} @4 in src/user.controller.base.ts",
            "serves http GET /users/{} @4 in src/user.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A global prefix reaches the controllers of imported modules"
)]
fn a_global_prefix_reaches_the_controllers_of_imported_modules() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-global-prefix")?;
    assert_eq!(unread(&json, "src/app.module.ts"), NOTHING);
    assert_eq!(unread(&json, "src/main.ts"), NOTHING);
    assert_eq!(
        sites("nestjs-global-prefix")?,
        ["serves http GET /api/users/{} @5 in src/user/user.controller.ts"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A global prefix reaches the routes a controller inherits"
)]
fn a_global_prefix_reaches_the_routes_a_controller_inherits() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("nestjs-global-prefix-inherited")?,
        [
            "serves http GET /api/users/{} @4 in src/user/base/user.controller.base.ts",
            "serves http GET /api/users/{} @4 in src/user/user.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "Modules that import each other through forwardRef keep the prefix"
)]
fn modules_that_import_each_other_through_forward_ref_keep_the_prefix() -> Result<(), Box<dyn Error>>
{
    assert_eq!(
        sites("nestjs-module-cycle")?,
        [
            "serves http GET /api/users/{} @5 in src/user/user.controller.ts",
            "serves http POST /api/login @5 in src/auth/auth.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A controller that no app reaches keeps its path")]
fn a_controller_that_no_app_reaches_keeps_its_path() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("nestjs-unreached-controller")?,
        [
            "serves http GET /api/users/{} @5 in src/user/user.controller.ts",
            "serves http GET /legacy/{} @5 in src/legacy/legacy.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A controller in a ring of modules that no app reaches keeps its path"
)]
fn a_controller_in_a_ring_of_modules_that_no_app_reaches_keeps_its_path()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("nestjs-unreached-ring")?,
        [
            "serves http GET /a/{} @5 in src/a.controller.ts",
            "serves http GET /b @4 in src/b.controller.base.ts",
            "serves http GET /b @4 in src/b.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A computed global prefix is an unread site")]
fn a_computed_global_prefix_is_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-computed-prefix")?;
    assert_eq!(
        unread(&json, "src/main.ts"),
        ["@7 non-literal app.setGlobalPrefix(prefix)"]
    );
    assert_eq!(
        sites("nestjs-computed-prefix")?,
        ["serves http GET /users/{} @5 in src/user/user.controller.ts"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A controller that a prefixed app and an unprefixed module both reach is served under both"
)]
fn a_controller_that_a_prefixed_app_and_an_unprefixed_module_both_reach_is_served_under_both()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("nestjs-two-apps")?,
        [
            "serves http GET /api/users/{} @5 in src/user/user.controller.ts",
            "serves http GET /users/{} @5 in src/user/user.controller.ts",
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A global prefix on an app made elsewhere is an unread site"
)]
fn a_global_prefix_on_an_app_made_elsewhere_is_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-global-prefix-elsewhere")?;
    assert_eq!(
        unread(&json, "src/setup.ts"),
        ["@4 unsupported app.setGlobalPrefix('api')"]
    );
    assert_eq!(
        unread(&json, "src/main.ts"),
        ["@6 unsupported app.setGlobalPrefix('api')"]
    );
    assert_eq!(
        sites("nestjs-global-prefix-elsewhere")?,
        ["serves http GET /users/{} @5 in src/user/user.controller.ts"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "The options of a global prefix are an unread site")]
fn the_options_of_a_global_prefix_are_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("nestjs-global-prefix-options")?;
    assert_eq!(
        unread(&json, "src/main.ts"),
        ["@6 unsupported app.setGlobalPrefix('api', { exclude: ['health'] })"]
    );
    assert_eq!(
        sites("nestjs-global-prefix-options")?,
        ["serves http GET /api/users/{} @5 in src/user/user.controller.ts"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A route behind a global prefix keeps the facts of other rules on its line"
)]
fn a_route_behind_a_global_prefix_keeps_the_facts_of_other_rules_on_its_line()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        contracts(
            &run("nestjs-global-prefix-on-one-line")?,
            "src/user.controller.ts"
        ),
        ["calls http * /c @5", "serves http GET /users/{} @5"]
    );
    assert_eq!(
        sites("nestjs-global-prefix-on-one-line")?,
        [
            "calls http * /c @5 in src/user.controller.ts",
            "serves http GET /api/users/{} @5 in src/user.controller.ts",
        ]
    );
    Ok(())
}
