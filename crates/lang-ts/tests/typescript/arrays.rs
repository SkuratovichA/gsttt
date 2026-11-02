//! An array of paths in a route decorator or an Express verb, read as one site for each path.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, sites, unread};

#[test]
#[scenario(
    "typescript",
    "Every path of a method decorator's array serves a route"
)]
fn every_path_of_a_method_decorators_array_serves_a_route() -> Result<(), Box<dyn Error>> {
    let json = run("array-paths-method")?;
    assert_eq!(
        sites("array-paths-method")?,
        [
            "serves http POST /hooks/webhook @5 in src/hooks.controller.ts",
            "serves http POST /hooks/webhook/{} @5 in src/hooks.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/hooks.controller.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A computed element of an array is an unread site and the other elements serve"
)]
fn a_computed_element_of_an_array_is_an_unread_site_and_the_other_elements_serve()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-computed")?;
    assert_eq!(
        sites("array-paths-computed")?,
        [
            "serves http POST /hooks/webhook @6 in src/hooks.controller.ts",
            "serves http POST /hooks/webhook/{} @6 in src/hooks.controller.ts",
        ]
    );
    assert_eq!(
        unread(&json, "src/hooks.controller.ts"),
        ["@6 non-literal Post"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An element of an array that is an imported constant waits for the link"
)]
fn an_element_of_an_array_that_is_an_imported_constant_waits_for_the_link()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-imported")?;
    assert_eq!(
        contracts(&json, "src/hooks.controller.ts"),
        ["serves http POST /hooks/webhook @6"]
    );
    assert_eq!(
        linked(&json),
        ["serves http POST /hooks/legacy/webhook @6 in src/hooks.controller.ts"]
    );
    assert_eq!(
        sites("array-paths-imported")?,
        [
            "serves http POST /hooks/legacy/webhook @6 in src/hooks.controller.ts",
            "serves http POST /hooks/webhook @6 in src/hooks.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/hooks.controller.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A controller with an array of prefixes serves every route under each"
)]
fn a_controller_with_an_array_of_prefixes_serves_every_route_under_each()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-controller")?;
    assert_eq!(
        sites("array-paths-controller")?,
        [
            "serves http GET /a/x @5 in src/items.controller.ts",
            "serves http GET /a/y @8 in src/items.controller.ts",
            "serves http GET /a/z @8 in src/items.controller.ts",
            "serves http GET /b/x @5 in src/items.controller.ts",
            "serves http GET /b/y @8 in src/items.controller.ts",
            "serves http GET /b/z @8 in src/items.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/items.controller.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A controller with a prefix that is read and one that is computed serves under the read one"
)]
fn a_controller_with_a_prefix_that_is_read_and_one_that_is_computed_serves_under_the_read_one()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-controller-computed")?;
    assert_eq!(
        sites("array-paths-controller-computed")?,
        ["serves http GET /a/x @6 in src/items.controller.ts"]
    );
    assert_eq!(
        unread(&json, "src/items.controller.ts"),
        ["@4 non-literal @Controller(['a', prefix()])"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A route of a base class is served under each prefix of a controller with an array"
)]
fn a_route_of_a_base_class_is_served_under_each_prefix_of_a_controller_with_an_array()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-base-class")?;
    assert_eq!(
        sites("array-paths-base-class")?,
        [
            "serves http GET /a/x @4 in src/items.controller.base.ts",
            "serves http GET /a/x @4 in src/items.controller.ts",
            "serves http GET /b/x @4 in src/items.controller.base.ts",
            "serves http GET /b/x @4 in src/items.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/items.controller.base.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An Express verb with an array of paths serves every path"
)]
fn an_express_verb_with_an_array_of_paths_serves_every_path() -> Result<(), Box<dyn Error>> {
    let json = run("array-paths-express")?;
    assert_eq!(
        sites("array-paths-express")?,
        [
            "serves http GET /a @4 in src/app.ts",
            "serves http GET /b @4 in src/app.ts",
        ]
    );
    assert_eq!(unread(&json, "src/app.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An empty array of paths is an unread site")]
fn an_empty_array_of_paths_is_an_unread_site() -> Result<(), Box<dyn Error>> {
    let json = run("array-paths-empty")?;
    assert_eq!(sites("array-paths-empty")?, NOTHING);
    assert_eq!(
        unread(&json, "src/hooks.controller.ts"),
        ["@5 non-literal Post"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_controller_prefix_in_an_options_object_is_read_for_its_first_path_only()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-options")?;
    assert_eq!(
        sites("array-paths-options")?,
        ["serves http GET /a/x @5 in src/items.controller.ts"]
    );
    assert_eq!(unread(&json, "src/items.controller.ts"), NOTHING);
    Ok(())
}

#[test]
fn an_imported_element_waits_for_the_link_under_every_prefix_and_in_an_express_verb()
-> Result<(), Box<dyn Error>> {
    let json = run("array-paths-deferred")?;
    // `LEGACY` is listed twice in the array of `create`, and serves once under each prefix.
    assert_eq!(
        sites("array-paths-deferred")?,
        [
            "serves http GET /a @5 in src/app.ts",
            "serves http GET /a/legacy @6 in src/items.controller.ts",
            "serves http GET /b/legacy @6 in src/items.controller.ts",
            "serves http GET /legacy @5 in src/app.ts",
            "serves http POST /a/legacy @9 in src/items.controller.ts",
            "serves http POST /a/x @9 in src/items.controller.ts",
            "serves http POST /b/legacy @9 in src/items.controller.ts",
            "serves http POST /b/x @9 in src/items.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/app.ts"), NOTHING);
    assert_eq!(unread(&json, "src/items.controller.ts"), NOTHING);
    Ok(())
}

/// `read_mount()` in `analyze.rs` reads the array of a mount by code of its own, and `Shape::each` reads the array of a verb: the two must find the same paths.
#[test]
fn a_mount_and_a_verb_read_the_same_paths_of_an_array() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        sites("array-paths-mount")?,
        [
            "serves http GET /a @7 in src/app.ts",
            "serves http GET /a/x @5 in src/app.ts",
            "serves http GET /b @7 in src/app.ts",
            "serves http GET /b/x @5 in src/app.ts",
        ]
    );
    Ok(())
}
