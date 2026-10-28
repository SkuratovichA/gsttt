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
#[scenario("typescript", "CommonJS exports are unknown")]
fn commonjs_exports_are_unknown() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports")?;
    for path in [
        "src/module-exports.js",
        "src/exports-member.js",
        "src/module-exports-member.js",
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
