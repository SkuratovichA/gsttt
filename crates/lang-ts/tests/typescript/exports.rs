//! What a file exports itself, when the connector cannot tell, and what a star passes on.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{affected_by, exported_as, exports, run, sites, status};

#[test]
#[scenario("typescript", "Every export form is named")]
fn every_export_form_is_named() -> Result<(), Box<dyn Error>> {
    let json = run("export-forms")?;
    assert_eq!(status(&json, "src/forms.ts"), "ok");
    assert_eq!(
        exports(&json, "src/forms.ts"),
        Some(vec![
            "C", "E", "I", "N", "T", "a", "b", "d", "default", "f", "g", "ns", "w", "y"
        ])
    );
    assert_eq!(
        exported_as(&json, "src/forms.ts"),
        ["./z z as w", "./ns * as ns"]
    );
    assert_eq!(
        exports(&json, "src/z.ts"),
        Some(vec!["z"]),
        "a file with one export names it"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "The names of a CommonJS file are listed")]
fn the_names_of_a_commonjs_file_are_listed() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-listed")?;
    assert_eq!(
        exports(&json, "src/object.js"),
        Some(vec!["a", "b", "c"]),
        "`module.exports = {{ a, b: x, c() {{}} }}`"
    );
    assert_eq!(
        exports(&json, "src/members.js"),
        Some(vec!["d", "e"]),
        "`exports.d = 1` and `module.exports.e = f`"
    );
    assert_eq!(
        exports(&json, "src/nested.js"),
        Some(vec!["deep"]),
        "`exports.a.b = 1` changes a member of `a` and exports no name"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A CommonJS file that exports one value exports default")]
fn a_commonjs_file_that_exports_one_value_exports_default() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-default")?;
    for path in [
        "src/form1.js",
        "src/form2.js",
        "src/form3.js",
        "src/form4.js",
        "src/named-class.js",
        "src/named-function.js",
        "src/named-arrow.js",
    ] {
        assert_eq!(exports(&json, path), Some(vec!["default"]), "{path}");
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "CommonJS exports that cannot be listed are unknown")]
fn commonjs_exports_that_cannot_be_listed_are_unknown() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-open")?;
    for path in [
        "src/spread.js",
        "src/computed.js",
        "src/variable.js",
        "src/required.js",
        "src/indexed.js",
        "src/module-indexed.js",
        "src/assigned.js",
        "src/factory.js",
        "src/defined.js",
        "src/export-equals.ts",
    ] {
        assert_eq!(status(&json, path), "ok", "{path}");
        assert_eq!(exports(&json, path), None, "{path}");
    }
    assert_eq!(
        exports(&json, "src/esm.ts"),
        Some(vec!["kept"]),
        "a module with export declarations stays known beside them"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "CommonJS exports are unknown")]
fn commonjs_exports_are_unknown() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-open")?;
    assert_eq!(exports(&json, "src/spread.js"), None);
    assert_eq!(exports(&json, "src/indexed.js"), None);
    Ok(())
}

#[test]
#[scenario("typescript", "The compiled module marker leaves the exports known")]
fn the_compiled_module_marker_leaves_the_exports_known() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-esmodule")?;
    assert_eq!(exports(&json, "src/compiled.js"), Some(vec!["a"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Exports of a file read past errors are unknown")]
fn exports_of_a_file_read_past_errors_are_unknown() -> Result<(), Box<dyn Error>> {
    let json = run("exports-past-errors")?;
    assert_eq!(status(&json, "src/rough.ts"), "recovered 1 @3");
    assert_eq!(exports(&json, "src/rough.ts"), None);
    assert_eq!(status(&json, "src/clean.ts"), "ok");
    assert_eq!(exports(&json, "src/clean.ts"), Some(vec!["c"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A star does not forward default to a mount")]
fn a_star_does_not_forward_default_to_a_mount() -> Result<(), Box<dyn Error>> {
    // The linker resolves the mounts against the same barrel the walk of `impact` reads, so the two must agree on what a star forwards.
    assert_eq!(
        sites("star-default")?,
        [
            "serves http GET /legacy @7 in src/users.ts",
            "serves http GET /users/{} @4 in src/users.ts",
        ],
        "`users` is found behind the star and `fallback` is not, so only the first is mounted"
    );
    assert_eq!(
        affected_by("star-default", "src/users.ts")?,
        [
            "src/routes.ts 1 re-exports users from src/users.ts",
            "src/by-default.ts 2 imports default from src/routes.ts",
            "src/by-name.ts 2 imports users from src/routes.ts",
        ],
        "an import of `default` takes a name the barrel does not export"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A cycle of stars ends")]
fn a_cycle_of_stars_ends_and_both_followers_find_the_name() -> Result<(), Box<dyn Error>> {
    // `a.ts` and `b.ts` re-export each other. The linker looks for a name that nothing exports round the cycle until its depth limit, and the walk meets `a.ts` again from the file it just left.
    assert_eq!(
        sites("star-cycle")?,
        ["serves http GET /users/{} @6 in src/b.ts"],
        "`users` is found behind the cycle and `gone` is not, so only the first is mounted"
    );
    assert_eq!(
        affected_by("star-cycle", "src/extra.ts")?,
        [
            "src/a.ts 1 re-exports extra from src/extra.ts",
            "src/b.ts 2 re-exports extra from src/a.ts",
            "src/by-gone.ts 2 imports gone from src/a.ts",
        ],
        "`a.ts` exports `users` through the cycle, so the importer of `users` is not reached, and it does not export `gone`"
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "`module.exports` of a local object lists the keys of the object"
)]
fn module_exports_of_a_local_object_lists_the_keys_of_the_object() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-local")?;
    assert_eq!(
        exports(&json, "src/codes.js"),
        Some(vec!["A", "B", "append", "default"])
    );
    assert_eq!(
        exports(&json, "src/spread-local.js"),
        None,
        "a local object with a spread names no more than it can"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An empty `module.exports` lists what is added to it")]
fn an_empty_module_exports_lists_what_is_added_to_it() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-local")?;
    assert_eq!(exports(&json, "src/empty.js"), Some(vec!["x", "y"]));
    assert_eq!(exports(&json, "src/only-empty.js"), Some(vec![]));
    Ok(())
}
