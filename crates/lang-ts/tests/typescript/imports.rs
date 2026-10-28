//! The forms of import, their flags, and what happens to a file with errors.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, imports, run, status, unread};

#[test]
#[scenario("typescript", "Every import form is read")]
fn every_import_form_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("import-forms")?;
    assert_eq!(
        imports(&json, "src/forms.ts"),
        [
            "./globals.d.ts reference @1 -> unresolved not-found",
            "./named static @2 [default,a,b] -> unresolved not-found",
            "./namespace static @3 [*] -> unresolved not-found",
            "./side-effect side-effect @4 -> unresolved not-found",
            "./types static @5 type-only [T] -> unresolved not-found",
            "./re-exported re-export @6 [x,y] -> unresolved not-found",
            "./all re-export-all @7 -> unresolved not-found",
            "./star re-export @8 [*] -> unresolved not-found",
            "./lazy dynamic @9 [*] -> unresolved not-found",
            "./cjs require @10 [default] -> unresolved not-found",
            "./legacy require @11 [default] -> unresolved not-found",
            "./present static @15 [ok] -> file src/present.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Type-only imports are flagged")]
fn type_only_imports_are_flagged() -> Result<(), Box<dyn Error>> {
    let json = run("type-only-imports")?;
    assert_eq!(
        imports(&json, "src/types.ts"),
        [
            "./a static @1 type-only [A] -> unresolved not-found",
            "./b static @2 [B,C] -> unresolved not-found",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A type query of a module is a type-only import")]
fn a_type_query_of_a_module_is_a_type_only_import() -> Result<(), Box<dyn Error>> {
    let json = run("type-query-imports")?;
    assert_eq!(
        imports(&json, "src/types.ts"),
        [
            "./lib static @1 type-only -> file src/lib.ts",
            "./lib static @2 type-only [helper] -> file src/lib.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An import type names the first segment of its qualifier"
)]
fn an_import_type_names_the_first_segment_of_its_qualifier() -> Result<(), Box<dyn Error>> {
    let json = run("qualified-import-types")?;
    assert_eq!(
        imports(&json, "src/types.ts"),
        [
            "./lib static @1 type-only [Editor] -> file src/lib.ts",
            "./lib static @2 type-only [Outer] -> file src/lib.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A nested import type is read at the line of its specifier"
)]
fn a_nested_import_type_is_read_at_the_line_of_its_specifier() -> Result<(), Box<dyn Error>> {
    let json = run("nested-import-types")?;
    assert_eq!(
        imports(&json, "src/types.ts"),
        [
            "./lib static @2 type-only -> file src/lib.ts",
            "./lib static @4 type-only [Box] -> file src/lib.ts",
            "./other static @4 type-only -> file src/other.ts",
            "./lib static @5 type-only -> file src/lib.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A computed import is a blind spot")]
fn a_computed_import_is_a_blind_spot() -> Result<(), Box<dyn Error>> {
    let json = run("computed-import")?;
    assert_eq!(
        unread(&json, "src/load.ts"),
        ["@2 non-literal import(name)"]
    );
    assert_eq!(imports(&json, "src/load.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A recoverable error keeps the imports")]
fn a_recoverable_error_keeps_the_imports() -> Result<(), Box<dyn Error>> {
    let json = run("recovered-error")?;
    assert_eq!(status(&json, "src/main.ts"), "recovered 1 @3");
    assert_eq!(
        imports(&json, "src/main.ts"),
        ["./a static @1 [a] -> file src/a.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn every_import_form_can_lead_to_a_file() -> Result<(), Box<dyn Error>> {
    let json = run("import-forms-resolved")?;
    let lines = imports(&json, "src/forms.ts");
    assert_eq!(lines.len(), 6, "{lines:?}");
    assert!(
        lines
            .iter()
            .all(|line| line.ends_with("-> file src/present.ts")),
        "{lines:?}"
    );
    Ok(())
}
