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
