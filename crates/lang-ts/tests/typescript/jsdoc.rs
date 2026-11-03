//! The types that the `JSDoc` comments of a JavaScript file give, which the compiler reads there.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, bound_names, imports, run, uses};

/// What the declaration `name` of `src/<file>` of the fixture `jsdoc-types` uses.
fn used(file: &str, name: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let json = run("jsdoc-types")?;
    let path = format!("src/{file}");
    let used = uses(&json, &path, name).ok_or("the file records no such declaration")?;
    Ok(used.into_iter().map(str::to_owned).collect())
}

fn has(used: &[String], name: &str) -> bool {
    used.iter().any(|item| item == name)
}

#[test]
#[scenario("typescript", "An @import in a JavaScript file is a type-only import")]
fn an_import_tag_in_a_javascript_file_is_a_type_only_import() -> Result<(), Box<dyn Error>> {
    let json = run("jsdoc-types")?;
    let all = imports(&json, "src/imports.js");
    assert!(
        all.contains(
            &"./config.js static @2 type-only [FlatConfigArray,Other] -> file src/config.js"
                .to_owned()
        ),
        "{all:?}"
    );
    assert!(
        all.contains(&"./ns.js static @3 type-only [*] -> file src/ns.js".to_owned()),
        "{all:?}"
    );
    assert!(
        all.contains(&"./def.js static @4 type-only [default] -> file src/def.js".to_owned()),
        "{all:?}"
    );
    let bound = bound_names(&json, "src/imports.js");
    assert!(
        bound.contains(
            &"./config.js @2 [FlatConfigArray,Other] as [FlatConfigArray,Renamed]".to_owned()
        ),
        "{bound:?}"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An import type in JSDoc is a type-only import")]
fn an_import_type_in_jsdoc_is_a_type_only_import() -> Result<(), Box<dyn Error>> {
    let json = run("jsdoc-types")?;
    let all = imports(&json, "src/imports.js");
    // The tags of one declaration that give the same import give one fact, at the line of the declaration.
    let client: Vec<&String> = all
        .iter()
        .filter(|import| import.starts_with("./client"))
        .collect();
    assert_eq!(
        client,
        [
            "./client static @14 type-only [default] -> file src/client.js",
            "./client.js static @10 type-only [Thing] -> file src/client.js",
        ]
    );
    assert!(
        all.contains(&"./other static @12 type-only [Wrapped] -> file src/other.js".to_owned()),
        "{all:?}"
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An import type with no name takes what the module exports as its value"
)]
fn an_import_type_with_no_name_takes_what_the_module_exports_as_its_value()
-> Result<(), Box<dyn Error>> {
    let json = run("jsdoc-types")?;
    let all = imports(&json, "src/imports.js");
    // `@typedef {import('./client')} Z` is `import Z from './other'` as a type.
    assert!(
        all.contains(&"./other static @16 type-only [default] -> file src/other.js".to_owned()),
        "{all:?}"
    );
    let bound = bound_names(&json, "src/imports.js");
    assert!(
        bound.contains(&"./other @16 [default] as [Z]".to_owned()),
        "{bound:?}"
    );
    // An import with no local is made once for each declaration that documents it, at the line of the declaration, and a name the code binds is not bound by a comment.
    let all = imports(&json, "src/types.js");
    let mut found: Vec<&String> = all
        .iter()
        .filter(|import| import.contains("type-only"))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            "./client static @10 type-only [default] -> file src/client.js",
            "./client static @13 type-only [default] -> file src/client.js",
            "./client.js static @4 type-only [Thing] -> file src/client.js",
            "./client.js static @7 type-only [Thing] -> file src/client.js",
        ]
    );
    assert!(used("types.js", "first")?.contains(&"Thing".to_owned()));
    Ok(())
}

#[test]
#[scenario("typescript", "A parameter documented with a class holds that class")]
fn a_parameter_documented_with_a_class_holds_that_class() -> Result<(), Box<dyn Error>> {
    for name in ["compareSpecificity", "optional", "arrow", "assigned"] {
        let used = used("params.js", name)?;
        assert!(has(&used, "Sel.compare"), "{name}: {used:?}");
    }
    for name in ["undocumented", "shadowed", "defaulted"] {
        let used = used("params.js", name)?;
        assert!(!has(&used, "Sel.compare"), "{name}: {used:?}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A destructured parameter documented by a dotted name holds that class"
)]
fn a_destructured_parameter_documented_by_a_dotted_name_holds_that_class()
-> Result<(), Box<dyn Error>> {
    let used_by_run = used("destructured.js", "run")?;
    assert!(has(&used_by_run, "Linter.verifyAndFix"), "{used_by_run:?}");
    assert!(
        has(&used_by_run, "ConfigLoader.loadConfigArrayForDirectory"),
        "{used_by_run:?}"
    );
    let chained = used("destructured.js", "chained")?;
    assert!(!has(&chained, "Linter.getFixPassCount"), "{chained:?}");
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A parameter of a method holds the class of its documentation"
)]
fn a_parameter_of_a_method_holds_the_class_of_its_documentation() -> Result<(), Box<dyn Error>> {
    for name in ["Traverser.run", "Traverser.each", "table"] {
        let used = used("methods.js", name)?;
        assert!(has(&used, "Visitor.callSync"), "{name}: {used:?}");
    }
    let skip = used("methods.js", "Traverser.skip")?;
    assert!(!has(&skip, "Visitor.callSync"), "{skip:?}");
    Ok(())
}

#[test]
#[scenario("typescript", "A type that is no plain class gives no class")]
fn a_type_that_is_no_plain_class_gives_no_class() -> Result<(), Box<dyn Error>> {
    let used = used("no-class.js", "f")?;
    for class in ["A", "B", "Foo", "sel", "Object", "Array"] {
        assert!(
            !used
                .iter()
                .any(|item| item.starts_with(&format!("{class}."))),
            "{class}: {used:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario("typescript", "JSDoc in a TypeScript file gives no fact")]
fn jsdoc_in_a_typescript_file_gives_no_fact() -> Result<(), Box<dyn Error>> {
    let json = run("jsdoc-types")?;
    assert_eq!(imports(&json, "src/typescript.ts"), NOTHING);
    assert_eq!(bound_names(&json, "src/typescript.ts"), NOTHING);
    let used = used("typescript.ts", "g")?;
    assert!(!has(&used, "Sel.compare"), "{used:?}");
    Ok(())
}

#[test]
#[scenario("typescript", "A code sample in JSDoc gives no fact")]
fn a_code_sample_in_jsdoc_gives_no_fact() -> Result<(), Box<dyn Error>> {
    let json = run("jsdoc-types")?;
    assert_eq!(imports(&json, "src/example.js"), NOTHING);
    Ok(())
}
