//! Names read by the scope they stand in: the constants of a function or a block, and the bindings that hide a name of the file or an import.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, linked, run, sites, unread};

/// What the file at `path` calls at `line`, as `<role> <key> @<line>`.
fn calls_at(json: &serde_json::Value, path: &str, line: u32) -> Vec<String> {
    let suffix = format!(" @{line}");
    contracts(json, path)
        .into_iter()
        .filter(|fact| fact.ends_with(&suffix))
        .collect()
}

/// The sites of the file at `path` at `line` that no rule could read, as `@<line> <reason> <detail>`.
fn unread_at(json: &serde_json::Value, path: &str, line: u32) -> Vec<String> {
    let prefix = format!("@{line} ");
    unread(json, path)
        .into_iter()
        .filter(|site| site.starts_with(&prefix))
        .collect()
}

#[test]
fn the_facts_of_the_constants_of_functions_are_snapshotted() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(run("scopes-locals")?);
    Ok(())
}

#[test]
#[scenario("typescript", "A function-local constant is read")]
fn a_function_local_constant_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-locals")?;
    assert_eq!(calls_at(&json, "src/api.ts", 5), ["calls http * /a @5"]);
    assert_eq!(unread_at(&json, "src/api.ts", 5), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A local built from another local and a constant of the file is read"
)]
fn a_local_built_from_another_local_and_a_constant_of_the_file_is_read()
-> Result<(), Box<dyn Error>> {
    let json = run("scopes-locals")?;
    assert_eq!(
        calls_at(&json, "src/api.ts", 11),
        ["calls http * /api/x/y @11"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "Two blocks that declare the same name read their own")]
fn two_blocks_that_declare_the_same_name_read_their_own() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-locals")?;
    assert_eq!(
        calls_at(&json, "src/api.ts", 17),
        ["calls http * /first @17"]
    );
    assert_eq!(
        calls_at(&json, "src/api.ts", 20),
        ["calls http * /second @20"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A closure reads a constant declared after it in the same scope"
)]
fn a_closure_reads_a_constant_declared_after_it_in_the_same_scope() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-locals")?;
    assert_eq!(
        calls_at(&json, "src/api.ts", 25),
        ["calls http * /later @25"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An inner function reads a constant of the function around it"
)]
fn an_inner_function_reads_a_constant_of_the_function_around_it() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-locals")?;
    assert_eq!(
        calls_at(&json, "src/api.ts", 33),
        ["calls http * /outer @33"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A local object constant is read by its property")]
fn a_local_object_constant_is_read_by_its_property() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-locals")?;
    assert_eq!(
        calls_at(&json, "src/api.ts", 40),
        ["calls http * /users @40"]
    );
    assert_eq!(
        calls_at(&json, "src/api.ts", 41),
        ["calls http * /orders @41"]
    );
    Ok(())
}

#[test]
fn the_facts_of_the_names_that_a_scope_binds_are_snapshotted() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(run("scopes-hidden")?);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A parameter hides a constant of the file of the same name"
)]
fn a_parameter_hides_a_constant_of_the_file_of_the_same_name() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-hidden")?;
    // The route of the constant is not stated for a site that never calls it.
    assert_eq!(calls_at(&json, "src/api.ts", 4), NOTHING);
    assert_eq!(unread_at(&json, "src/api.ts", 4), ["@4 non-literal fetch"]);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A local constant hides a constant of the file of the same name"
)]
fn a_local_constant_hides_a_constant_of_the_file_of_the_same_name() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-hidden")?;
    assert_eq!(calls_at(&json, "src/api.ts", 9), ["calls http * /local @9"]);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A let and a destructured name hide a constant of the file"
)]
fn a_let_and_a_destructured_name_hide_a_constant_of_the_file() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-hidden")?;
    for line in [14, 19] {
        assert_eq!(calls_at(&json, "src/api.ts", line), NOTHING, "@{line}");
        assert_eq!(
            unread_at(&json, "src/api.ts", line),
            [format!("@{line} non-literal fetch")]
        );
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A var in a block hides a constant of the file in the whole function"
)]
fn a_var_in_a_block_hides_a_constant_of_the_file_in_the_whole_function()
-> Result<(), Box<dyn Error>> {
    let json = run("scopes-hidden")?;
    // The `var` is in a block of the function and the call below the block, yet the call reads the `var`.
    assert_eq!(calls_at(&json, "src/api.ts", 26), NOTHING);
    assert_eq!(
        unread_at(&json, "src/api.ts", 26),
        ["@26 non-literal fetch"]
    );
    // Outside the function the constant is the name.
    assert_eq!(calls_at(&json, "src/api.ts", 38), ["calls http * /api @38"]);
    Ok(())
}

#[test]
#[scenario("typescript", "A type that names a parameter hides nothing")]
fn a_type_that_names_a_parameter_hides_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-hidden")?;
    for line in [31, 35] {
        assert_eq!(
            calls_at(&json, "src/api.ts", line),
            [format!("calls http * /api @{line}")]
        );
    }
    Ok(())
}

#[test]
fn the_facts_of_a_client_made_in_a_function_are_snapshotted() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(run("scopes-client")?);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A client made in a function reads the base it is given there"
)]
fn a_client_made_in_a_function_reads_the_base_it_is_given_there() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-client")?;
    assert_eq!(
        calls_at(&json, "src/api.ts", 8),
        ["calls http GET /api/users/list @8"]
    );
    // A parameter named like a constant of the file gives the client no base, so it is called at the path after it.
    assert_eq!(
        calls_at(&json, "src/api.ts", 13),
        ["calls http GET /orders @13"]
    );
    Ok(())
}

#[test]
fn a_route_path_held_in_a_function_local_constant_is_served() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-routes")?;
    assert_eq!(linked(&json), ["serves http GET /health @7 in src/app.ts"]);
    // A parameter is no constant.
    assert_eq!(
        unread_at(&json, "src/app.ts", 8),
        ["@8 non-literal app.get"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A parameter named like an imported client is no client")]
fn a_parameter_named_like_an_imported_client_is_no_client() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-receiver-client")?;
    // The call in `list` is on the parameter, so only the call outside it links through the import.
    assert_eq!(
        linked(&json),
        ["calls http GET /api/orders @9 in src/users.ts"]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A parameter named like an imported router is no mount")]
fn a_parameter_named_like_an_imported_router_is_no_mount() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-receiver-mount")?;
    // The router stays unmounted, so it serves at the path it is written with and not under `/api`.
    assert_eq!(
        linked(&json),
        ["serves http GET /users @5 in src/router.ts"]
    );
    assert_eq!(
        sites("scopes-receiver-mount")?,
        ["serves http GET /users @5 in src/router.ts"]
    );
    assert_eq!(contracts(&json, "src/setup.ts"), NOTHING);
    assert_eq!(unread(&json, "src/setup.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An element of an array of paths reads a constant of its scope"
)]
fn an_element_of_an_array_of_paths_reads_a_constant_of_its_scope() -> Result<(), Box<dyn Error>> {
    let json = run("scopes-array")?;
    assert_eq!(
        sites("scopes-array")?,
        [
            "serves http GET /a @7 in src/routes.ts",
            "serves http GET /x @7 in src/routes.ts",
        ]
    );
    assert_eq!(unread(&json, "src/routes.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}
