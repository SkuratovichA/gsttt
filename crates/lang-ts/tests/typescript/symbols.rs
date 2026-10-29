//! The declarations of a file, what each one uses, what the file runs as it loads, and the local names of its imports and exports.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{
    NOTHING, declared, edges, import_line, local_exports, local_names, on_load, run, status, uses,
};

#[test]
#[scenario("typescript", "Every declaration form is recorded")]
fn every_declaration_form_is_recorded() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-declarations")?;
    assert_eq!(status(&json, "src/forms.ts"), "ok");
    assert_eq!(
        declared(&json, "src/forms.ts"),
        [
            "f @1",
            "g @3-7",
            "a @9",
            "c @9",
            "C @11-20",
            "C.constructor @12",
            "C.m @13",
            "C.s @14",
            "C.#p @15",
            "C.v @16-19",
            "E @22",
            "I @24-29",
            "T @31",
            "N @33",
            "default @35",
        ],
        "overloads and merged declarations are joined, a getter and its setter are one member, and `obj` is read, not declared"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A file read past errors records no declarations")]
fn a_file_read_past_errors_records_no_declarations() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-past-errors")?;
    assert_eq!(status(&json, "src/rough.ts"), "recovered 1 @5");
    assert_eq!(declared(&json, "src/rough.ts"), NOTHING);
    assert_eq!(on_load(&json, "src/rough.ts"), None);
    assert_eq!(
        local_names(&json, "src/rough.ts"),
        NOTHING,
        "a static import has no local name to carry without declarations"
    );
    assert_eq!(status(&json, "src/clean.ts"), "ok");
    assert_eq!(declared(&json, "src/clean.ts"), ["foo @3"]);
    assert_eq!(local_names(&json, "src/clean.ts"), ["./dep dep"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "References, tags, types and this are uses")]
fn references_tags_types_and_this_are_uses() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-uses")?;
    let path = "src/view.tsx";
    assert_eq!(
        uses(&json, path, "View"),
        Some(vec!["Props", "Widget", "helper", "useX"]),
        "a type, a tag, a call and an import"
    );
    assert_eq!(uses(&json, path, "C.a"), Some(vec!["C.b"]));
    assert_eq!(uses(&json, path, "d"), Some(vec!["C", "C.s"]));
    assert_eq!(uses(&json, path, "helper"), Some(vec![]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A function that fills a cache is a use of the cache")]
fn a_function_that_fills_a_cache_is_a_use_of_the_cache() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-writes")?;
    let path = "src/cache.ts";
    assert_eq!(uses(&json, path, "cache"), Some(vec!["fill"]));
    assert_eq!(uses(&json, path, "count"), Some(vec!["bump"]));
    assert_eq!(
        uses(&json, path, "read"),
        Some(vec!["cache"]),
        "a reader refers to the cache, which is the other direction"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "What runs at load is recorded")]
fn what_runs_at_load_is_recorded() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-load")?;
    let path = "src/main.ts";
    assert_eq!(
        on_load(&json, path),
        Some(vec!["D", "f", "h", "k", "mixin", "test"])
    );
    assert_eq!(
        uses(&json, path, "a"),
        Some(vec!["f"]),
        "a call is both at load and a use of what it initializes"
    );
    assert_eq!(
        uses(&json, path, "b"),
        Some(vec!["g"]),
        "a name that is only read is a use and not at load"
    );
    assert_eq!(uses(&json, path, "C.m"), Some(vec!["n"]));
    assert_eq!(uses(&json, path, "C.s"), Some(vec!["k"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Imports and exports record their local names")]
fn imports_and_exports_record_their_local_names() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-names")?;
    let path = "src/names.ts";
    assert_eq!(
        local_names(&json, path),
        ["react React", "./ns ns", "./x b, c", "./t T"]
    );
    assert_eq!(local_exports(&json, path), ["default from f", "g from f"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_default_export_is_named_default_unless_it_names_what_it_declares() -> Result<(), Box<dyn Error>>
{
    let json = run("symbols-default-forms")?;

    let path = "src/anonymous-class.ts";
    assert_eq!(
        declared(&json, path),
        [
            "default @1-7",
            "default.s @2",
            "default.m @3-5",
            "default.n @6"
        ],
        "the members of a class with no name are members of `default`"
    );
    assert_eq!(edges(&json, path), ["default.m -> default.n"]);

    let path = "src/named-class.ts";
    assert_eq!(
        declared(&json, path),
        ["Named @1-6", "Named.m @2-4", "Named.n @5"]
    );
    assert_eq!(edges(&json, path), ["Named.m -> Named.n"]);
    assert_eq!(
        local_exports(&json, path),
        ["default from Named"],
        "the export `default` has the class for its local"
    );

    let path = "src/named-function.ts";
    assert_eq!(declared(&json, path), ["named @1"]);
    assert_eq!(local_exports(&json, path), ["default from named"]);

    for path in ["src/anonymous-function.ts", "src/arrow.ts"] {
        assert_eq!(
            edges(&json, path),
            ["default -> helper"],
            "{path}: a function waits for a call"
        );
        assert_eq!(on_load(&json, path), Some(vec![]), "{path}");
        assert_eq!(local_exports(&json, path), NOTHING, "{path}");
    }

    let path = "src/expression.ts";
    assert_eq!(declared(&json, path), ["default @3"]);
    assert_eq!(edges(&json, path), ["default -> make, part"]);
    assert_eq!(
        on_load(&json, path),
        Some(vec!["make", "part"]),
        "a call in the expression of a default export runs as the file loads"
    );

    let path = "src/identifier.ts";
    assert_eq!(
        declared(&json, path),
        ["value @1"],
        "`export default value` declares nothing: it exports a name the file has"
    );
    assert_eq!(local_exports(&json, path), ["default from value"]);
    assert_eq!(on_load(&json, path), Some(vec![]));
    Ok(())
}

#[test]
fn a_member_with_a_computed_key_belongs_to_its_class() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-members")?;
    let path = "src/members.ts";
    assert_eq!(
        declared(&json, path),
        [
            "Keys @3-11",
            "Keys.plain @10",
            "Nested @13-23",
            "Nested.m @14-20",
            "Nested.x @21",
            "Nested.y @22",
            "Privates @25-35",
            "Privates.#secret @26",
            "Privates.#hidden @27",
            "Privates.reveal @28-31",
            "Privates.make @32-34",
            "Statics @37-46",
            "Statics.count @38",
            "Statics.bump @42-44",
            "shadow @48-50",
        ],
        "a computed key and a static block declare no member"
    );
    let uses = edges(&json, path);
    assert!(
        uses.contains(&"Keys -> helper, other".to_owned()),
        "the code under a computed key is the class's: {uses:?}"
    );
    assert!(
        uses.contains(&"Statics -> helper, other".to_owned()),
        "a static block and a computed static field are the class's: {uses:?}"
    );
    assert_eq!(
        on_load(&json, path),
        Some(vec!["helper"]),
        "a static block runs as the class is defined, and a field reads `other`, which calls nothing"
    );
    Ok(())
}

#[test]
fn this_in_a_nested_function_is_a_use_of_the_member_of_the_class_around()
-> Result<(), Box<dyn Error>> {
    let json = run("symbols-members")?;
    let path = "src/members.ts";
    let uses = edges(&json, path);
    assert!(
        uses.contains(&"Nested.m -> Nested.x, Nested.y".to_owned()),
        "a function declaration, which has a `this` of its own, is taken for the class's: a use too many beats a use missed; {uses:?}"
    );
    Ok(())
}

#[test]
fn a_private_name_is_a_member_of_the_class_that_declares_it() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-members")?;
    let path = "src/members.ts";
    let uses = edges(&json, path);
    assert!(
        uses.contains(&"Privates.reveal -> Privates.#hidden, Privates.#secret".to_owned()),
        "`this.#hidden` and `#secret in this`: {uses:?}"
    );
    assert!(
        uses.contains(&"Privates.make -> Privates, Privates.#secret".to_owned()),
        "`instance.#secret` names the private field of the class around it: {uses:?}"
    );
    assert!(
        uses.contains(&"Statics.bump -> Statics, Statics.count".to_owned()),
        "`Statics.count` is a use of both, and so is `this.count` of the member: {uses:?}"
    );
    Ok(())
}

#[test]
fn shadowing_is_ignored() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-members")?;
    let uses = edges(&json, "src/members.ts");
    assert!(
        uses.contains(&"shadow -> helper".to_owned()),
        "a parameter named like an import is a use of the import, which can only add: {uses:?}"
    );
    Ok(())
}

#[test]
fn a_declaration_spans_the_lines_of_its_statement_and_its_decorators() -> Result<(), Box<dyn Error>>
{
    let json = run("symbols-lines")?;
    let path = "src/lines.ts";
    assert_eq!(
        declared(&json, path),
        [
            "first @3-6",
            "renamed @3-6",
            "one @8",
            "two @9",
            "Decorated @11-18",
            "Decorated.field @13-14",
            "Decorated.method @16-17",
            "gen @20-22",
            "arrow @24-26",
        ],
        "a destructuring spans its statement, a declarator that shares its statement spans itself, and a decorator is part of what it decorates"
    );
    assert_eq!(
        edges(&json, path),
        [
            "Decorated -> decorate",
            "Decorated.field -> track",
            "Decorated.method -> track",
            "gen -> first",
            "arrow -> renamed",
        ]
    );
    assert_eq!(
        on_load(&json, path),
        Some(vec!["decorate", "track"]),
        "a decorator is evaluated as the class is defined"
    );
    Ok(())
}

#[test]
fn a_dynamic_import_in_a_method_lies_within_the_lines_of_the_method() -> Result<(), Box<dyn Error>>
{
    let json = run("symbols-dynamic-import")?;
    let path = "src/lazy.ts";
    assert_eq!(declared(&json, path), ["Lazy @1-5", "Lazy.load @2-4"]);
    assert_eq!(
        import_line(&json, path, "./target"),
        Some(3),
        "the walk attributes the import to the narrowest declaration whose lines hold it"
    );
    assert_eq!(
        local_names(&json, path),
        NOTHING,
        "a dynamic import binds no name"
    );
    Ok(())
}

#[test]
fn a_test_file_uses_what_it_tests_at_load() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-vitest")?;
    let path = "test/hash.test.ts";
    assert_eq!(declared(&json, path), ["runner @5", "check @7-9"]);
    assert_eq!(edges(&json, path), ["runner -> Runner", "check -> getHash"]);
    assert_eq!(
        on_load(&json, path),
        Some(vec![
            "Runner", "check", "describe", "expect", "getHash", "it", "runner", "vi"
        ]),
        "what a callback of `describe` or `it` calls runs as the file loads"
    );
    assert_eq!(
        local_names(&json, path),
        [
            "vitest describe, expect, it, vi",
            "../src/runner Runner",
            "../src/utils getHash"
        ]
    );
    Ok(())
}

#[test]
fn every_form_of_a_write_is_a_use_of_the_variable() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-write-forms")?;
    assert_eq!(
        edges(&json, "src/forms.ts"),
        [
            "table -> assignMember",
            "list -> pushes",
            "seen -> adds",
            "nested -> deep",
            "total -> compound",
            "current -> loops",
            "first -> destructures",
            "second -> destructures",
            "shorthand -> shorthands",
            "doomed -> removes",
            "assignMember -> table",
            "pushes -> list",
            "adds -> seen",
            "deep -> nested",
            "reads -> untouched",
            "compound -> total",
            "loops -> current",
            "destructures -> first, second",
            "shorthands -> shorthand",
            "removes -> doomed",
        ],
        "`untouched` is only read, so it lists no writer"
    );
    Ok(())
}

#[test]
fn every_form_that_runs_as_the_file_loads_is_recorded() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-load-forms")?;
    let path = "src/forms.ts";
    assert_eq!(
        on_load(&json, path),
        Some(vec!["Inject", "a", "c", "f", "g", "h", "i", "j", "m", "n"]),
        "a call in an enum or a namespace, a function called on the spot, a tagged template, `new`, a value of an object and the decorator of a parameter run as the file loads; a read, a function and a method do not"
    );
    assert_eq!(
        edges(&json, path),
        [
            "Level -> a, b",
            "Space -> c, d, e",
            "iife -> f",
            "tagged -> g, h",
            "made -> i, j",
            "lazy -> k",
            "object -> l, m",
            "Service.constructor -> Dep, Inject, n",
        ]
    );
    Ok(())
}
