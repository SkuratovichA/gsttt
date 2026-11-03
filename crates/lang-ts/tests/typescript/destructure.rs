//! A destructuring of a module takes the names it writes, and not the module whole.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{run, uses};

#[test]
#[scenario("typescript", "A destructured namespace uses the members it names")]
fn a_destructured_namespace_uses_the_members_it_names() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-namespace")?;
    assert_eq!(
        uses(&json, "src/use.ts", "take"),
        Some(vec!["lib.a", "lib.b"])
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A destructured require uses the members it names")]
fn a_destructured_require_uses_the_members_it_names() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-required")?;
    assert_eq!(uses(&json, "src/use.js", "take"), Some(vec!["lib.a"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A destructured default import uses the members it names"
)]
fn a_destructured_default_import_uses_the_members_it_names() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-default")?;
    assert_eq!(uses(&json, "src/use.js", "take"), Some(vec!["utils.a"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A destructuring that may take any name uses the module whole"
)]
fn a_destructuring_that_may_take_any_name_uses_the_module_whole() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-whole")?;
    assert_eq!(uses(&json, "src/use.ts", "takeRest"), Some(vec!["lib"]));
    assert_eq!(uses(&json, "src/use.ts", "takeComputed"), Some(vec!["lib"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A destructuring inside a function counts for that function"
)]
fn a_destructuring_inside_a_function_counts_for_that_function() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-function")?;
    assert_eq!(uses(&json, "src/use.ts", "first"), Some(vec!["lib.a"]));
    assert_eq!(uses(&json, "src/use.ts", "second"), Some(vec![]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A default value in a destructuring is still read")]
fn a_default_value_in_a_destructuring_is_still_read() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-default-value")?;
    assert_eq!(
        uses(&json, "src/use.ts", "take"),
        Some(vec!["helper", "lib.a"])
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A destructuring of a local object changes nothing")]
fn a_destructuring_of_a_local_object_changes_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("destructure-local")?;
    assert_eq!(uses(&json, "src/use.ts", "take"), Some(vec!["config"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}
