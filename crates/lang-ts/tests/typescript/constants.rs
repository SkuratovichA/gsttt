//! Constants read where a rule reads text: in the file that declares them, and in the files that import them.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, sites, unread};

#[test]
#[scenario("typescript", "A constant in the same file is read")]
fn a_constant_in_the_same_file_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("constants-same-file")?;
    assert_eq!(
        contracts(&json, "src/api.ts"),
        ["calls http * /api/users @4", "calls http * /api/orders @5"]
    );
    assert_eq!(unread(&json, "src/api.ts"), NOTHING);
    Ok(())
}

#[test]
fn every_form_of_constant_is_read_and_nothing_else() -> Result<(), Box<dyn Error>> {
    let json = run("constants-forms")?;
    assert_eq!(
        contracts(&json, "src/forms.ts"),
        [
            "calls http * /a/b @16",
            "calls http * /a/c @17",
            "calls http * /d @18",
            "calls http * /e @19",
            "calls http * /g @21",
            "calls http * /i @23",
            "calls http * /j @24",
            "calls http * /l @26",
            "calls http * /a/b/{} @28",
            "calls http * /last @31",
        ]
    );
    assert_eq!(
        unread(&json, "src/forms.ts"),
        [
            "@20 non-literal fetch",
            "@22 non-literal fetch",
            "@25 non-literal fetch",
            "@27 non-literal fetch",
            "@30 non-literal fetch",
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An imported enum member names an environment variable")]
fn an_imported_enum_member_names_an_environment_variable() -> Result<(), Box<dyn Error>> {
    let json = run("constants-enum-env")?;
    assert_eq!(
        linked(&json),
        ["calls env API_URL @10 in src/app.service.ts"]
    );
    assert_eq!(unread(&json, "src/app.service.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A static property and a nested object property are read"
)]
fn a_static_property_and_a_nested_object_property_are_read() -> Result<(), Box<dyn Error>> {
    let json = run("constants-static-nested")?;
    let mut served = linked(&json);
    served.sort();
    assert_eq!(
        served,
        [
            "serves http GET /users/login @6 in src/users.controller.ts",
            "serves http GET /users/{} @9 in src/users.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/users.controller.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A constant imported through re-exports or a namespace is read"
)]
fn a_constant_imported_through_re_exports_or_a_namespace_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("constants-reexport")?;
    let mut served = linked(&json);
    served.sort();
    assert_eq!(
        served,
        [
            "serves http GET /login @6 in src/barrel.controller.ts",
            "serves http GET /login @6 in src/namespace.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/barrel.controller.ts"), NOTHING);
    assert_eq!(unread(&json, "src/namespace.controller.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_constant_exported_under_another_name_or_by_default_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("constants-export-forms")?;
    let mut served = linked(&json);
    served.sort();
    assert_eq!(
        served,
        [
            "serves http GET /home @9 in src/a.controller.ts",
            "serves http GET /login @6 in src/a.controller.ts",
        ]
    );
    assert_eq!(unread(&json, "src/a.controller.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An imported constant path of an Express router is served under its mount"
)]
fn an_imported_constant_path_of_an_express_router_is_served_under_its_mount()
-> Result<(), Box<dyn Error>> {
    let json = run("constants-express")?;
    assert_eq!(
        linked(&json),
        ["serves http GET /v1/users @5 in src/users.ts"]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An imported constant path of a controller is served behind the global prefix"
)]
fn an_imported_constant_path_of_a_controller_is_served_behind_the_global_prefix()
-> Result<(), Box<dyn Error>> {
    let json = run("constants-global-prefix")?;
    assert_eq!(
        sites("constants-global-prefix")?,
        ["serves http GET /api/auth/login @6 in src/auth.controller.ts"]
    );
    assert_eq!(
        linked(&json),
        ["serves http GET /api/auth/login @6 in src/auth.controller.ts"]
    );
    assert_eq!(unread(&json, "src/auth.controller.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An imported constant path on a base class is served by its controllers"
)]
fn an_imported_constant_path_on_a_base_class_is_served_by_its_controllers()
-> Result<(), Box<dyn Error>> {
    let json = run("constants-base-class")?;
    assert_eq!(
        sites("constants-base-class")?,
        [
            "serves http GET /users/user @4 in src/user.controller.ts",
            "serves http GET /users/user @5 in src/user.controller.base.ts",
        ]
    );
    assert_eq!(unread(&json, "src/user.controller.base.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A constant is read wherever the parse reads text")]
fn a_constant_is_read_wherever_the_parse_reads_text() -> Result<(), Box<dyn Error>> {
    let json = run("constants-uses")?;
    assert_eq!(
        sites("constants-uses")?,
        [
            "calls http GET /v1/x @6 in src/client.ts",
            "serves http GET /api/users/x @7 in src/users.controller.ts",
            "serves http GET /v2/y @8 in src/server.ts",
        ]
    );
    for path in [
        "src/main.ts",
        "src/users.controller.ts",
        "src/client.ts",
        "src/server.ts",
    ] {
        assert_eq!(unread(&json, path), NOTHING, "{path}");
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A controller whose prefix is not read serves nothing")]
fn a_controller_whose_prefix_is_not_read_serves_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("constants-unread-prefix")?;
    assert_eq!(
        unread(&json, "src/auth.controller.ts"),
        ["@4 non-literal @Controller(PREFIX)"]
    );
    assert_eq!(
        unread(&json, "src/user.controller.ts"),
        ["@5 non-literal @Controller(prefix())"]
    );
    assert_eq!(contracts(&json, "src/auth.controller.ts"), NOTHING);
    assert_eq!(contracts(&json, "src/user.controller.ts"), NOTHING);
    assert_eq!(linked(&json), NOTHING);
    // The base class serves no route of its own, and none through the controller that extends it.
    assert_eq!(contracts(&json, "src/user.controller.base.ts"), NOTHING);
    // A controller that gives no prefix is not one that gives an unread prefix.
    assert_eq!(unread(&json, "src/health.controller.ts"), NOTHING);
    assert_eq!(
        sites("constants-unread-prefix")?,
        ["serves http GET /health @5 in src/health.controller.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A name that is no constant leaves the site unread")]
fn a_name_that_is_no_constant_leaves_the_site_unread() -> Result<(), Box<dyn Error>> {
    let json = run("constants-not-constant")?;
    assert_eq!(
        unread(&json, "src/api.ts"),
        ["@5 non-literal fetch", "@6 non-literal fetch"]
    );
    assert_eq!(contracts(&json, "src/api.ts"), NOTHING);
    assert_eq!(sites("constants-not-constant")?, NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}
