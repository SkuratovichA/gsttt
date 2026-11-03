//! The forms of import, their flags, and what happens to a file with errors.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{
    NOTHING, bound_names, declared, imports, run, status, tests_run_by, unread, uses,
};

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
            "./lib static @1 type-only [*] -> file src/lib.ts",
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
            "./lib static @2 type-only [*] -> file src/lib.ts",
            "./lib static @4 type-only [Box] -> file src/lib.ts",
            "./other static @4 type-only [*] -> file src/other.ts",
            "./lib static @5 type-only [*] -> file src/lib.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mocked module is a mock")]
fn a_mocked_module_is_a_mock() -> Result<(), Box<dyn Error>> {
    let json = run("mocked-modules")?;
    let expected: Vec<String> = (1..=11)
        .map(|line| format!("./lib mock @{line} -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/runners.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A factory that does not use its parameter replaces the module"
)]
fn a_factory_that_does_not_use_its_parameter_replaces_the_module() -> Result<(), Box<dyn Error>> {
    let json = run("replacing-mocks")?;
    let expected: Vec<String> = (1..=3)
        .map(|line| format!("./lib mock @{line} replaces -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/replacing.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A factory that uses its parameter does not replace the module"
)]
fn a_factory_that_uses_its_parameter_does_not_replace_the_module() -> Result<(), Box<dyn Error>> {
    let json = run("partial-mocks")?;
    let expected: Vec<String> = (1..=3)
        .map(|line| format!("./lib mock @{line} -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/partial.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mock without a factory does not replace the module")]
fn a_mock_without_a_factory_does_not_replace_the_module() -> Result<(), Box<dyn Error>> {
    let json = run("bare-mocks")?;
    let expected: Vec<String> = (1..=3)
        .map(|line| format!("./lib mock @{line} -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/automock.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A nested mock call does not replace the module")]
fn a_nested_mock_call_does_not_replace_the_module() -> Result<(), Box<dyn Error>> {
    let json = run("nested-mocks")?;
    assert_eq!(
        imports(&json, "src/nested.test.ts"),
        [
            "./lib mock @2 -> file src/lib.ts",
            "./lib mock @5 -> file src/lib.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mock that is not hoisted does not replace the module")]
fn a_mock_that_is_not_hoisted_does_not_replace_the_module() -> Result<(), Box<dyn Error>> {
    let json = run("late-mocks")?;
    let expected: Vec<String> = (1..=3)
        .map(|line| format!("./lib mock @{line} -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/late.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

/// The tests of `unsure-mocks` that replace `src/lib.ts` with a factory and also name it in another call of `vi` or `jest`, so that they may run its code. `src/replaced.test.ts` replaces it and does nothing else.
const UNSURE: [&str; 13] = [
    "create-mock",
    "deep-unmock",
    "do-mock",
    "dont-mock",
    "gen-mock",
    "import-mock",
    "jest-unmock",
    "mocked-twice",
    "require-mock",
    "set-mock",
    "unstable-mock",
    "vi-do-unmock",
    "vi-unmock",
];

#[test]
fn a_test_that_also_names_a_replaced_module_in_another_mock_call_is_listed()
-> Result<(), Box<dyn Error>> {
    let expected = UNSURE.map(|name| format!("src/{name}.test.ts"));

    let mut tests = tests_run_by("unsure-mocks", "src/dep.ts")?;
    tests.sort();

    assert_eq!(
        tests, expected,
        "only src/replaced.test.ts runs none of src/lib.ts"
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A load named at run time leaves no mock replacing its module"
)]
fn a_load_named_at_run_time_leaves_no_mock_replacing_its_module() -> Result<(), Box<dyn Error>> {
    let json = run("named-loads")?;
    for name in [
        "import-name",
        "require-name",
        "import-actual-name",
        "require-actual-name",
    ] {
        assert_eq!(
            imports(&json, &format!("src/{name}.test.ts")),
            ["./lib mock @3 -> file src/lib.ts"],
            "{name}"
        );
    }
    assert_eq!(
        imports(&json, "src/replaced.test.ts"),
        ["./lib mock @1 replaces -> file src/lib.ts"]
    );
    Ok(())
}

#[test]
fn a_test_that_loads_a_module_named_at_run_time_is_listed() -> Result<(), Box<dyn Error>> {
    let mut tests = tests_run_by("named-loads", "src/dep.ts")?;
    tests.sort();

    assert_eq!(
        tests,
        [
            "src/import-actual-name.test.ts",
            "src/import-name.test.ts",
            "src/require-actual-name.test.ts",
            "src/require-name.test.ts",
        ],
        "only src/replaced.test.ts loads nothing that may be src/lib.ts"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An actual module is a dynamic import")]
fn an_actual_module_is_a_dynamic_import() -> Result<(), Box<dyn Error>> {
    let json = run("actual-modules")?;
    let expected: Vec<String> = (1..=2)
        .map(|line| format!("./lib dynamic @{line} actual -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/actual.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mock made from a module or set for it is a mock")]
fn a_mock_made_from_a_module_or_set_for_it_is_a_mock() -> Result<(), Box<dyn Error>> {
    let json = run("mocks-from-modules")?;
    let expected: Vec<String> = (1..=5)
        .map(|line| format!("./lib mock @{line} -> file src/lib.ts"))
        .collect();
    assert_eq!(imports(&json, "src/made.test.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A module named at run time is no import")]
fn a_module_named_at_run_time_is_no_import() -> Result<(), Box<dyn Error>> {
    let json = run("computed-mocks")?;
    assert_eq!(imports(&json, "src/computed.test.ts"), NOTHING);
    assert_eq!(
        unread(&json, "src/computed.test.ts"),
        ["@5 non-literal import(name)"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mock call on another receiver is no import")]
fn a_mock_call_on_another_receiver_is_no_import() -> Result<(), Box<dyn Error>> {
    let json = run("foreign-mock-calls")?;
    assert_eq!(imports(&json, "src/others.test.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A mock that asks for the actual module reaches it")]
fn a_mock_that_asks_for_the_actual_module_reaches_it() -> Result<(), Box<dyn Error>> {
    let json = run("mocks-with-actual")?;
    assert_eq!(
        imports(&json, "src/lib.test.ts"),
        [
            "./lib mock @1 replaces -> file src/lib.ts",
            "./lib dynamic @2 actual -> file src/lib.ts",
            "./lib static @2 type-only [*] -> file src/lib.ts",
            "./editor mock @5 -> file src/editor.ts",
            "./editor static @6 type-only [*] -> file src/editor.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read from a loaded module is the name it takes"
)]
fn a_member_read_from_a_loaded_module_is_the_name_it_takes() -> Result<(), Box<dyn Error>> {
    let json = run("loaded-members")?;
    assert_eq!(
        imports(&json, "src/use.ts"),
        [
            "./lib dynamic @1 [helper] -> file src/lib.ts",
            "./lib require @2 [other] -> file src/lib.ts",
            "./lib dynamic @3 -> file src/lib.ts",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read from an actual module is the name it takes"
)]
fn a_member_read_from_an_actual_module_is_the_name_it_takes() -> Result<(), Box<dyn Error>> {
    let json = run("actual-members")?;
    assert_eq!(
        imports(&json, "src/members.test.ts"),
        [
            "./lib dynamic @1 actual [helper] -> file src/lib.ts",
            "./lib dynamic @2 actual [other] -> file src/lib.ts",
            "./lib dynamic @3 actual [third] -> file src/lib.ts",
            "./lib dynamic @4 actual -> file src/lib.ts",
        ]
    );
    assert_eq!(unread(&json, "src/members.test.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A destructuring that may take more than it names takes the module"
)]
fn a_destructuring_that_may_take_more_than_it_names_takes_the_module() -> Result<(), Box<dyn Error>>
{
    let json = run("loaded-open")?;
    assert_eq!(
        imports(&json, "src/open.ts"),
        [
            "./lib dynamic @1 [a,*] -> file src/lib.ts",
            "./lib dynamic @2 [*] -> file src/lib.ts",
            "./lib require @3 [c] -> file src/lib.ts",
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
fn a_member_is_read_from_a_load_through_wrappers_and_not_through_a_promise()
-> Result<(), Box<dyn Error>> {
    let json = run("loaded-member-forms")?;
    assert_eq!(
        imports(&json, "src/forms.ts"),
        [
            "./lib dynamic @1 [a] -> file src/lib.ts",
            "./lib dynamic @2 [b] -> file src/lib.ts",
            "./lib require @3 [c] -> file src/lib.ts",
            "./lib dynamic @4 -> file src/lib.ts",
            "./lib dynamic @5 -> file src/lib.ts",
        ]
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

#[test]
#[scenario("typescript", "A require at the top level binds locals")]
fn a_require_at_the_top_level_binds_locals() -> Result<(), Box<dyn Error>> {
    let json = run("require-binds")?;
    let path = "src/user.js";
    assert_eq!(
        bound_names(&json, path),
        ["./a @1 [default] as [lib]", "./a @2 [b,c] as [b,d]"]
    );
    assert_eq!(
        declared(&json, path),
        ["run @4-6"],
        "`lib`, `b` and `d` declare nothing, as the names of an import do"
    );
    assert_eq!(
        uses(&json, path, "run"),
        Some(vec!["b", "d", "lib.x"]),
        "`lib.x` is a use of `x` of `lib`"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A require that cannot bind stays as it was")]
fn a_require_that_cannot_bind_stays_as_it_was() -> Result<(), Box<dyn Error>> {
    let json = run("require-unbound")?;
    let path = "src/user.js";
    assert_eq!(
        bound_names(&json, path),
        [
            "./a @1 [a,default] as []",
            "./a @2 [a] as []",
            "./a @3 [a] as []",
            "./a @6 [default] as []",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A require in a conditional binds its names")]
fn a_require_in_a_conditional_binds_its_names() -> Result<(), Box<dyn Error>> {
    let json = run("require-forms")?;
    let lines = bound_names(&json, "src/user.js");
    for wanted in [
        "./a @1 [A,B] as [A,B]",
        "./b @2 [default] as [lib]",
        "./c @2 [default] as [lib]",
    ] {
        assert!(
            lines.iter().any(|line| line == wanted),
            "{wanted}: {lines:?}"
        );
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A require assigned to a local of the file takes its name"
)]
fn a_require_assigned_to_a_local_of_the_file_takes_its_name() -> Result<(), Box<dyn Error>> {
    let json = run("require-forms")?;
    let lines = bound_names(&json, "src/user.js");
    for wanted in ["./d @6 [Linter] as []", "./g @7 [default] as []"] {
        assert!(
            lines.iter().any(|line| line == wanted),
            "{wanted}: {lines:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario("typescript", "A require read by a computed member binds the module")]
fn a_require_read_by_a_computed_member_binds_the_module() -> Result<(), Box<dyn Error>> {
    let json = run("require-forms")?;
    let lines = bound_names(&json, "src/user.js");
    for wanted in ["./e @9 [default] as [pick]", "./f @10 [default] as [other]"] {
        assert!(
            lines.iter().any(|line| line == wanted),
            "{wanted}: {lines:?}"
        );
    }
    Ok(())
}
