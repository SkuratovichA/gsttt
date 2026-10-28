//! What a file exports itself, and when the connector cannot tell.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{exported_as, exports, run, status};

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
