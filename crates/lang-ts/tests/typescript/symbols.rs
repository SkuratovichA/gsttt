//! The declarations of a file, what each one uses, what the file runs as it loads, and the local names of its imports and exports.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{
    NOTHING, declared, edges, heirs, holds, import_line, imports, local_exports, local_names,
    on_load, overrides, pairs, reads, run, status, use_lines, uses,
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
            "I.first @25",
            "I.second @28",
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
        Some(vec!["Props.", "Widget", "helper", "useX"]),
        "a type, a tag, a call and an import"
    );
    assert_eq!(uses(&json, path, "C.a"), Some(vec!["C.b"]));
    assert_eq!(uses(&json, path, "d"), Some(vec!["C.s"]));
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
        Some(vec!["cache.get"]),
        "a reader refers to the cache, which is the other direction, and a `Map` is a value that takes the members of its class"
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
        uses.contains(&"Privates.make -> Privates, Privates., Privates.#secret".to_owned()),
        "`instance.#secret` names the private field of the class around it: {uses:?}"
    );
    assert!(
        uses.contains(&"Statics.bump -> Statics.count".to_owned()),
        "`Statics.count` and `this.count` are uses of the member alone: {uses:?}"
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
            "Runner", "check", "describe", "expect", "getHash", "it", "runner", "vi.fn"
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
            "adds -> seen.add",
            "deep -> nested",
            "reads -> untouched.get",
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
            "Service.constructor -> Dep., Inject, n",
        ]
    );
    Ok(())
}

#[test]
fn an_import_type_binds_no_name() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-import-type")?;
    let path = "src/types.ts";
    assert_eq!(declared(&json, path), ["Row @1", "Whole @2", "make @3"]);
    assert_eq!(edges(&json, path), ["make -> Row"]);
    assert_eq!(
        imports(&json, path),
        [
            "./lib static @1 type-only [Row] -> file src/lib.ts",
            "./lib static @2 type-only [*] -> file src/lib.ts",
        ],
        "the import types are static imports of the file, as before"
    );
    assert_eq!(
        local_names(&json, path),
        NOTHING,
        "an import type binds no local name, so the walk cannot follow it by name"
    );
    Ok(())
}

#[test]
fn ambient_blocks_and_import_aliases_are_recorded_conservatively() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-ambient")?;
    let path = "src/ambient.ts";
    assert_eq!(
        declared(&json, path),
        ["Outer @13-15", "Alias @17", "read @19-21"]
    );
    assert_eq!(edges(&json, path), ["Alias -> Outer", "read -> Alias"]);
    assert_eq!(
        on_load(&json, path),
        Some(vec!["use"]),
        "what `declare module` and `declare global` refer to counts as used at load, as a block that is not a declaration of the file"
    );
    assert_eq!(local_names(&json, path), ["./lib use"]);
    Ok(())
}

#[test]
fn import_equals_require_declares_its_name_as_const_require_does() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-import-equals")?;
    let path = "src/equals.ts";
    assert_eq!(
        declared(&json, path),
        ["x @1", "run @3-5"],
        "the name holds the lines of the require, which is how the walk gives the require to `x`"
    );
    assert_eq!(
        import_line(&json, path, "./lib"),
        Some(1),
        "the require lies within the lines of `x`"
    );
    assert_eq!(
        edges(&json, path),
        ["run -> x"],
        "`run` uses `x`, and `x` uses nothing, itself included"
    );
    assert_eq!(
        on_load(&json, path),
        Some(vec![]),
        "the require refers to no name of the file"
    );

    let same = "src/const.ts";
    assert_eq!(
        declared(&json, same),
        ["run @3-5"],
        "`const x = require('./lib')` binds a module as an import does, and declares nothing"
    );
    assert_eq!(
        on_load(&json, path),
        on_load(&json, same),
        "neither require refers to a name of the file"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_type_written_in_a_member_chain_is_a_use() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-chain-types")?;
    let path = "src/chains.ts";
    assert_eq!(
        edges(&json, path),
        [
            "viaAs -> AsType",
            "viaSatisfies -> SatisfiesType",
            "viaAssertion -> AssertionType",
            "viaInstantiation -> InstantiatedType, make",
            "viaBoth -> InnerType, OuterType",
            "viaNonNull -> NonNullType",
            "viaLongChain -> DeepType",
            "viaRoot -> RootType, root",
        ],
        "a cast, `satisfies`, an assertion and an instantiation keep their type whatever wraps them and however long the chain is, and the chain still goes through them: `root` is a use, a parameter is not"
    );
    assert_eq!(
        on_load(&json, path),
        Some(vec![]),
        "a function waits for a call"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn export_default_interface_exports_the_interface_by_its_name() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-default-interface")?;
    let path = "src/shape.ts";
    assert_eq!(declared(&json, path), ["Foo @1-3", "Foo.n @2"]);
    assert_eq!(
        local_exports(&json, path),
        ["default from Foo"],
        "the export `default` has the interface for its local, as it has a named class or function"
    );

    let importer = "src/consumer.ts";
    assert_eq!(
        imports(&json, importer),
        ["./shape static @1 [default] -> file src/shape.ts"]
    );
    assert_eq!(local_names(&json, importer), ["./shape Foo"]);
    assert_eq!(edges(&json, importer), ["make -> Foo"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A method that calls an injected service uses the member it calls"
)]
fn a_method_that_calls_an_injected_service_uses_the_member_it_calls() -> Result<(), Box<dyn Error>>
{
    let json = run("symbols-injected-fields")?;
    let path = "src/invoices.resolver.ts";
    assert_eq!(
        uses(&json, path, "InvoiceResolver.issue"),
        Some(vec![
            "InvoiceResolver.jobs",
            "InvoiceResolver.mail",
            "InvoiceService.issue",
            "JobQueue.add",
            "MailQueue.push",
        ]),
        "`this.invoices.issue()` uses `issue` of the class of the field, and not the class"
    );
    assert_eq!(
        uses(&json, path, "InvoiceResolver.other"),
        Some(vec![]),
        "a parameter without a modifier is no field, so other uses none of them"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A field read as a value uses its class")]
fn a_field_read_as_a_value_uses_its_class() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-injected-fields")?;
    assert_eq!(
        uses(&json, "src/invoices.resolver.ts", "InvoiceResolver.all"),
        Some(vec!["InvoiceService"])
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A member of a field is read in its own class")]
fn a_member_of_a_field_is_read_in_its_own_class() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-field-own-class")?;
    let path = "src/jobs.ts";
    assert_eq!(
        uses(&json, path, "Mailer.send"),
        Some(vec!["MailQueue.push"])
    );
    assert_eq!(
        uses(&json, path, "Billing"),
        Some(vec![]),
        "a field that no method reads uses nothing"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A member of a private field is read through the field")]
fn a_method_that_reads_a_private_field_uses_its_type() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-private-fields")?;
    let path = "src/mailer.ts";
    assert_eq!(
        uses(&json, path, "Mailer.send"),
        Some(vec!["MailQueue.push", "Mailer.#queue"])
    );
    assert_eq!(
        uses(&json, path, "Mailer.later"),
        Some(vec!["JobQueue.add", "Mailer.#jobs"])
    );
    // `queue` and `#queue` are two fields, and only the private one has a type.
    let count = uses(&json, path, "Mailer.count").ok_or("no Mailer.count")?;
    assert!(!count.contains(&"MailQueue"), "{count:?}");
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A member is used as the type of the field is written")]
fn a_member_is_used_as_the_type_of_the_field_is_written() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-field-members")?;
    let path = "src/notifier.ts";
    let of = |name| uses(&json, path, name);
    assert_eq!(
        of("Notifier.qualified"),
        Some(vec!["services.MailQueue.push"]),
        "a qualified type keeps its qualifier"
    );
    assert_eq!(
        of("Notifier.union"),
        Some(vec!["JobQueue.send", "MailQueue.send"]),
        "each type of a union has the member"
    );
    assert_eq!(
        of("Notifier.declared"),
        Some(vec!["Local.run"]),
        "a class of the file with the member is used through it"
    );
    assert_eq!(
        of("Notifier.undeclared"),
        Some(vec!["Local.start"]),
        "a member the class does not declare is used by its name all the same"
    );
    assert_eq!(
        of("Notifier.nested"),
        Some(vec!["services.MailQueue.items"]),
        "only the first member read from the field is the member"
    );
    assert_eq!(
        of("Notifier.indexed"),
        Some(vec!["services.MailQueue"]),
        "a computed key names no member, so the field is read as a value, and its type is used by its name"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A member is used however the field is spelled")]
fn a_member_is_used_however_the_field_is_spelled() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-field-spellings")?;
    let path = "src/spellings.ts";
    let of = |name: &str| uses(&json, path, &format!("Spellings.{name}"));
    for name in ["optional", "nonNull", "quoted", "bound"] {
        assert_eq!(of(name), Some(vec!["MailQueue.push"]), "{name}");
    }
    assert_eq!(
        of("cast"),
        Some(vec!["MailQueue.push", "Special"]),
        "the type of a cast is a use, as in any chain"
    );
    for name in ["destructured", "alias"] {
        assert_eq!(
            of(name),
            Some(vec!["MailQueue"]),
            "{name}: a field taken as a value is read through no member"
        );
    }
    assert_eq!(
        of("intersection"),
        Some(vec!["Left.send", "Right.send"]),
        "each type of an intersection has the member"
    );
    assert_eq!(
        of("privateCast"),
        Some(vec!["MailQueue.push", "Special", "Spellings.#queue"]),
        "the type of a cast around a private field is a use too"
    );
    assert_eq!(
        of("privateNonNull"),
        Some(vec!["MailQueue.push", "Spellings.#queue"])
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

/// The declarations of `src/uses.ts` of the fixture `member-uses`.
fn member_uses(name: &str) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let json = run("member-uses")?;
    Ok(uses(&json, "src/uses.ts", name).map(|used| used.into_iter().map(str::to_owned).collect()))
}

#[test]
#[scenario(
    "typescript",
    "A member read from a new instance is a use of that member"
)]
fn a_member_read_from_a_new_instance_is_a_use_of_that_member() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        member_uses("refund")?,
        Some(vec![
            "OrderService.cancel".into(),
            "OrderService.constructor".into()
        ])
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read from a constant made by new is a use of that member"
)]
fn a_member_read_from_a_constant_made_by_new_is_a_use_of_that_member() -> Result<(), Box<dyn Error>>
{
    assert_eq!(
        member_uses("refundLater")?,
        Some(vec![
            "OrderService.cancel".into(),
            "OrderService.constructor".into()
        ])
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read from a parameter of a class type is a use of that member"
)]
fn a_member_read_from_a_parameter_of_a_class_type_is_a_use_of_that_member()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        member_uses("refundWith")?,
        Some(vec!["OrderService.".into(), "OrderService.cancel".into()])
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A value of a class read as a value is a use of the class"
)]
fn a_value_of_a_class_read_as_a_value_is_a_use_of_the_class() -> Result<(), Box<dyn Error>> {
    for name in ["pass", "keep"] {
        let used = member_uses(name)?.unwrap_or_default();
        assert!(
            used.iter().any(|used| used == "OrderService"),
            "{name}: {used:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A static member of an imported class is a use of that member"
)]
fn a_static_member_of_an_imported_class_is_a_use_of_that_member() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        member_uses("create")?,
        Some(vec!["OrderService.create".into()])
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A static member of a class of the file is a use of that member"
)]
fn a_static_member_of_a_class_of_the_file_is_a_use_of_that_member() -> Result<(), Box<dyn Error>> {
    assert_eq!(member_uses("begin")?, Some(vec!["Runner.start".into()]));
    Ok(())
}

#[test]
#[scenario("typescript", "A type with arguments holds no class")]
fn a_type_with_arguments_holds_no_class() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        member_uses("load")?,
        Some(vec!["Repository".into(), "User".into()])
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A name bound twice in a scope holds no class")]
fn a_name_bound_twice_in_a_scope_holds_no_class() -> Result<(), Box<dyn Error>> {
    let used = member_uses("twice")?.ok_or("the file records no `twice`")?;
    assert!(used.iter().any(|used| used == "OrderService"), "{used:?}");
    assert!(
        !used.iter().any(|used| used.ends_with(".cancel")),
        "{used:?}"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A qualified type name is used by its whole name")]
fn a_qualified_type_name_is_used_by_its_whole_name() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-qualified")?;
    let path = "src/types.ts";
    assert_eq!(uses(&json, path, "typed"), Some(vec!["lib.Foo"]));
    assert_eq!(uses(&json, path, "wrapped"), Some(vec!["lib.Bar"]));
    assert_eq!(uses(&json, path, "queried"), Some(vec!["lib.helper"]));
    assert_eq!(uses(&json, path, "onlyType"), Some(vec!["T.Foo"]));
    assert_eq!(
        uses(&json, path, "escapes"),
        Some(vec!["lib.Foo"]),
        "a value of the type that goes elsewhere is a use of the type, not of the namespace"
    );
    assert_eq!(
        uses(&json, path, "passed"),
        Some(vec!["consume", "lib.Foo"])
    );
    assert_eq!(uses(&json, path, "escapesType"), Some(vec!["T.Foo"]));
    assert_eq!(
        uses(&json, path, "unbound"),
        Some(vec![]),
        "a root that is no binding records nothing"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A JSX member is used by its whole name")]
fn a_jsx_member_is_used_by_its_whole_name() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-qualified")?;
    assert_eq!(uses(&json, "src/view.tsx", "jsx"), Some(vec!["lib.Comp"]));
    Ok(())
}

#[test]
#[scenario("typescript", "A class records what it extends")]
fn a_class_records_what_it_extends() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-heirs")?;
    let all = heirs(&json, "src/classes.ts");
    for pair in ["Plain of Base", "Qualified of lib.Base", "Named of Base"] {
        assert!(all.iter().any(|heir| heir == pair), "{pair}: {all:?}");
    }
    for name in ["Mixed", "Alone", "Promised"] {
        assert!(
            !all.iter().any(|heir| heir.starts_with(&format!("{name} "))),
            "{name} takes no head: {all:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario("typescript", "An interface records what it extends")]
fn an_interface_records_what_it_extends() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-heirs")?;
    let all = heirs(&json, "src/classes.ts");
    for pair in ["Wide of Base", "Wide of lib.Shape", "Named of Base"] {
        assert!(all.iter().any(|heir| heir == pair), "{pair}: {all:?}");
    }
    Ok(())
}

#[test]
#[scenario("typescript", "A top-level value records the class it holds")]
fn a_top_level_value_records_the_class_it_holds() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-heirs")?;
    assert_eq!(
        heirs(&json, "src/classes.ts"),
        [
            "Named of Base",
            "Plain of Base",
            "Qualified of lib.Base",
            "Wide of Base",
            "Wide of lib.Shape",
            "generic of Repository",
            "made of Service",
            "typed of Service",
        ],
        "`let`, `var`, an unknown value and a value below the top level hold nothing"
    );
    assert_eq!(heirs(&json, "src/default.ts"), ["default of Service"]);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "`super` and an inherited `this` name the member of the head"
)]
fn super_and_an_inherited_this_name_the_member_of_the_head() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-heirs")?;
    let path = "src/members.ts";
    assert_eq!(
        uses(&json, path, "Sub.place"),
        Some(vec!["Base.own", "Base.place", "Sub.own"])
    );
    assert_eq!(
        uses(&json, path, "Sub.constructor"),
        Some(vec!["Base.constructor"])
    );
    assert_eq!(
        uses(&json, path, "Headless.run"),
        Some(vec![]),
        "a class with no head records nothing for `this` and `super`"
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read from a name with a head is used by its name"
)]
fn a_member_read_from_a_name_with_a_head_is_used_by_its_name() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-heirs")?;
    let path = "src/members.ts";
    assert_eq!(uses(&json, path, "use"), Some(vec!["orders.place"]));
    assert_eq!(uses(&json, path, "build"), Some(vec!["Sub.make"]));
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A file records the members it reads on a value of unknown class"
)]
fn a_file_records_the_members_it_reads_on_a_value_of_unknown_class() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-reads")?;
    assert_eq!(
        reads(&json, "src/unknown.tsx"),
        ["a", "b", "c", "d", "e", "g", "h", "i", "j", "k", "spyOn"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A read of a known class or of the class itself records no name"
)]
fn a_read_of_a_known_class_or_of_the_class_itself_records_no_name() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-reads")?;
    assert_eq!(reads(&json, "src/known.ts"), NOTHING);
    assert_eq!(pairs(&json, "src/known.ts"), ["repo.save"]);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member of a field of a class below the top level is read on an unknown value"
)]
fn a_member_of_a_field_of_a_class_below_the_top_level_is_read_on_an_unknown_value()
-> Result<(), Box<dyn Error>> {
    let json = run("symbols-reads-nested")?;
    assert_eq!(reads(&json, "src/nested.ts"), ["update"]);
    Ok(())
}

#[test]
#[scenario("typescript", "A member read on a global builtin records no name")]
fn a_member_read_on_a_global_builtin_records_no_name() -> Result<(), Box<dyn Error>> {
    let json = run("reads-builtin")?;
    assert_eq!(reads(&json, "src/global.js"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A key of an object literal the file wrote is no read")]
fn a_key_of_an_object_literal_the_file_wrote_is_no_read() -> Result<(), Box<dyn Error>> {
    let json = run("reads-literal")?;
    for ext in ["ts", "js"] {
        assert_eq!(
            reads(&json, &format!("src/mock.{ext}")),
            ["mockResolvedValueOnce", "mockReturnValueOnce"],
            "mock.{ext}"
        );
        for file in ["method", "exported"] {
            assert_eq!(
                reads(&json, &format!("src/{file}.{ext}")),
                NOTHING,
                "{file}.{ext}"
            );
        }
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read below a key that holds another value is a read"
)]
fn a_member_read_below_a_key_that_holds_another_value_is_a_read() -> Result<(), Box<dyn Error>> {
    let json = run("reads-literal")?;
    for ext in ["ts", "js"] {
        for (file, expected) in [
            ("held", vec!["place"]),
            ("made", vec!["place"]),
            ("nested", vec!["b", "other"]),
        ] {
            assert_eq!(
                reads(&json, &format!("src/{file}.{ext}")),
                expected,
                "{file}.{ext}"
            );
        }
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A spread, a computed key, a cast or a destructuring hides no read"
)]
fn a_spread_a_computed_key_a_cast_or_a_destructuring_hides_no_read() -> Result<(), Box<dyn Error>> {
    let json = run("reads-literal")?;
    for ext in ["ts", "js"] {
        for file in ["spread", "computed", "assigned"] {
            assert_eq!(
                reads(&json, &format!("src/{file}.{ext}")),
                ["place"],
                "{file}.{ext}"
            );
        }
    }
    assert_eq!(reads(&json, "src/cast.ts"), ["other", "place", "third"]);
    assert_eq!(
        reads(&json, "src/destructured.ts"),
        ["findMany", "resource"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A name bound again gives no shape")]
fn a_name_bound_again_gives_no_shape() -> Result<(), Box<dyn Error>> {
    let json = run("reads-literal")?;
    for ext in ["ts", "js"] {
        for file in ["reassigned", "parameter", "inner"] {
            assert_eq!(
                reads(&json, &format!("src/{file}.{ext}")),
                ["place"],
                "{file}.{ext}"
            );
        }
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read on a proxy or on globalThis records the name"
)]
fn a_member_read_on_a_proxy_or_on_global_this_records_the_name() -> Result<(), Box<dyn Error>> {
    let json = run("reads-builtin")?;
    assert_eq!(reads(&json, "src/proxy.js"), ["held", "hung", "proxied"]);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read on a global name the file binds records the name"
)]
fn a_member_read_on_a_global_name_the_file_binds_records_the_name() -> Result<(), Box<dyn Error>> {
    let json = run("reads-builtin")?;
    assert_eq!(reads(&json, "src/shadow.js"), ["entries", "from"]);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read on a Node module or on a value made by one records no name"
)]
fn a_member_read_on_a_node_module_or_on_a_value_made_by_one_records_no_name()
-> Result<(), Box<dyn Error>> {
    let json = run("reads-builtin")?;
    // `normalize`, `close`, `listen` and `set` are the reads on `path`, `server`, `second` and `late`. `createServer` is the destructuring itself, which reads the name on the module as it always did.
    assert_eq!(
        reads(&json, "src/node.js"),
        [
            "createServer",
            "madeRead",
            "mixedRead",
            "notNode",
            "parameterRead"
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An indexed type names the member")]
fn an_indexed_type_names_the_member() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-reads")?;
    assert_eq!(
        uses(&json, "src/known.ts", "P"),
        Some(vec!["Service.place"])
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A function written in place on exports is declared")]
fn a_function_written_in_place_on_exports_is_declared() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-functions")?;
    let path = "src/lib.js";
    assert_eq!(
        declared(&json, path),
        [
            "dep @2",
            "run @4-6",
            "stop @8-10",
            "a @12-14",
            "b @12-14",
            "limit @16",
            "label @18",
            "compute @23-25",
            "again @27-29",
        ],
        "a function, an arrow and a chain declare over their statement, a value declares like a const, and a local or a require declares nothing"
    );
    assert_eq!(
        uses(&json, path, "run"),
        Some(vec!["dep", "helper.go"]),
        "what the body uses counts for `run`"
    );
    assert_eq!(uses(&json, path, "a"), Some(vec!["dep"]));
    assert_eq!(uses(&json, path, "label"), Some(vec!["compute", "dep"]));
    assert_eq!(
        on_load(&json, path),
        Some(vec!["compute", "dep"]),
        "only the call that runs at load counts, not the bodies of `run` and `stop`"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A call on exports is a use of the name the file declares"
)]
fn a_call_on_exports_is_a_use_of_the_name_the_file_declares() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-functions")?;
    assert_eq!(uses(&json, "src/lib.js", "again"), Some(vec!["run"]));
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An object assigned to module.exports declares each property written in place"
)]
fn an_object_assigned_to_module_exports_declares_each_property_written_in_place()
-> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-object")?;
    let path = "src/lib.js";
    assert_eq!(
        declared(&json, path),
        [
            "dep @3",
            "other @4",
            "a @5",
            "run @8-10",
            "stop @11-13",
            "limit @14",
            "Bar @15-19",
            "Bar.go @16-18",
        ]
    );
    assert_eq!(uses(&json, path, "run"), Some(vec!["dep"]));
    assert_eq!(uses(&json, path, "stop"), Some(vec!["other"]));
    assert_eq!(uses(&json, path, "Bar.go"), Some(vec!["dep"]));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A property that holds a local or a require declares nothing"
)]
fn a_property_that_holds_a_local_or_a_require_declares_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-object")?;
    let path = "src/lib.js";
    let declared = declared(&json, path);
    for name in ["b ", "c ", "d "] {
        assert!(
            !declared.iter().any(|line| line.starts_with(name)),
            "{name}: {declared:?}"
        );
    }
    assert_eq!(
        on_load(&json, path),
        Some(vec!["a", "c", "dep", "local"]),
        "the names the properties hold are read as the file loads, and so is the method of the computed key, which declares nothing"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A computed key declares nothing")]
fn a_computed_key_declares_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-object")?;
    let declared = declared(&json, "src/lib.js");
    assert!(
        !declared.iter().any(|line| line.starts_with("computed")),
        "{declared:?}"
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A class written in place on exports is declared under its exported name"
)]
fn a_class_written_in_place_on_exports_is_declared_under_its_exported_name()
-> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-class")?;
    let path = "src/named.js";
    assert_eq!(
        declared(&json, path),
        ["dep @1", "Foo @3-7", "Foo.run @4-6"]
    );
    assert_eq!(uses(&json, path, "Foo.run"), Some(vec!["dep"]));
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A class assigned to module.exports is declared and exported as default"
)]
fn a_class_assigned_to_module_exports_is_declared_and_exported_as_default()
-> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-class")?;
    let path = "src/whole.js";
    assert_eq!(declared(&json, path), ["dep @1", "Foo @3-7", "Foo.go @4-6"]);
    assert_eq!(local_exports(&json, path), ["default from Foo"]);
    assert_eq!(declared(&json, "src/fn.js"), ["run @1-3"]);
    assert_eq!(local_exports(&json, "src/fn.js"), ["default from run"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An anonymous function assigned to module.exports declares nothing"
)]
fn an_anonymous_function_assigned_to_module_exports_declares_nothing() -> Result<(), Box<dyn Error>>
{
    let json = run("commonjs-declared-class")?;
    assert_eq!(declared(&json, "src/anonymous.js"), ["dep @1"]);
    assert_eq!(on_load(&json, "src/anonymous.js"), Some(vec!["dep"]));
    Ok(())
}

#[test]
#[scenario("typescript", "A void placeholder on exports is no declaration")]
fn a_void_placeholder_on_exports_is_no_declaration() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-declared-void")?;
    assert_eq!(declared(&json, "src/compiled.js"), ["a @3-5"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A member of an interface is a declaration")]
fn a_member_of_an_interface_is_a_declaration() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-interfaces")?;
    assert_eq!(
        declared(&json, "src/provider.ts"),
        [
            "Provider @1-7",
            "Provider.name @2",
            "Provider.list @3-4",
            "Named @9-11",
            "Named.id @10",
            "Shape @13-15"
        ],
        "overloads are one member, a computed key and an index signature declare none, and a type alias stays one declaration"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A class records what it extends and implements")]
fn a_class_records_what_it_extends_and_implements() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-overrides")?;
    assert_eq!(
        overrides(&json, "src/classes.ts"),
        ["Aws of Base", "Aws of Provider", "Aws of lib.Other"]
    );
    Ok(())
}

/// What the declaration `name` of `src/uses.ts` of the fixture `factory-members` uses.
fn factory_uses(path: &str, name: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let json = run("factory-members")?;
    let used = uses(&json, path, name).ok_or("the file records no such declaration")?;
    Ok(used.into_iter().map(str::to_owned).collect())
}

#[test]
#[scenario(
    "typescript",
    "A member read from a constant made by a static call is a use of that member"
)]
fn a_member_read_from_a_constant_made_by_a_static_call_is_a_use_of_that_member()
-> Result<(), Box<dyn Error>> {
    let used = factory_uses("src/uses.ts", "viaConstant")?;
    assert!(used.contains(&"Service.place".to_owned()), "{used:?}");
    assert!(!used.contains(&"Service.other".to_owned()), "{used:?}");
    assert!(!used.contains(&"Service".to_owned()), "{used:?}");
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read from the result of a static call is a use of that member"
)]
fn a_member_read_from_the_result_of_a_static_call_is_a_use_of_that_member()
-> Result<(), Box<dyn Error>> {
    for name in ["viaInstance", "viaAwait", "viaParentheses"] {
        let used = factory_uses("src/uses.ts", name)?;
        assert!(
            used.contains(&"Service.place".to_owned()),
            "{name}: {used:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario("typescript", "A chain of calls stays on the class")]
fn a_chain_of_calls_stays_on_the_class() -> Result<(), Box<dyn Error>> {
    for name in ["viaChain", "viaConstantChain"] {
        let used = factory_uses("src/uses.ts", name)?;
        for member in ["FixTracker.retain", "FixTracker.remove"] {
            assert!(used.contains(&member.to_owned()), "{name}: {used:?}");
        }
    }
    let used = factory_uses("src/uses.ts", "viaStaticChain")?;
    for member in ["Service.with", "Service.place"] {
        assert!(used.contains(&member.to_owned()), "{used:?}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A call on a lower-case name or a namespace import names no class"
)]
fn a_call_on_a_lower_case_name_or_a_namespace_import_names_no_class() -> Result<(), Box<dyn Error>>
{
    let json = run("factory-members")?;
    let read = reads(&json, "src/uses.ts");
    for member in ["info", "place"] {
        assert!(read.iter().any(|read| read == member), "{member}: {read:?}");
    }
    for name in ["viaLowerCase", "viaNamespace"] {
        let used = factory_uses("src/uses.ts", name)?;
        assert!(
            used.iter().all(|used| !used.starts_with("Service.")),
            "{name}: {used:?}"
        );
    }
    let used = factory_uses("src/required.js", "viaRequiredLower")?;
    assert!(
        used.iter().all(|used| !used.starts_with("Service.")),
        "viaRequiredLower: {used:?}"
    );
    assert!(read.iter().any(|read| read == "place"), "{read:?}");
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A static call on a class bound by require gives that class"
)]
fn a_static_call_on_a_class_bound_by_require_gives_that_class() -> Result<(), Box<dyn Error>> {
    let used = factory_uses("src/required.js", "viaRequire")?;
    assert!(used.contains(&"Service.place".to_owned()), "{used:?}");
    let json = run("factory-members")?;
    let all = heirs(&json, "src/required.js");
    assert!(
        all.iter().any(|heir| heir == "shared of Service"),
        "{all:?}"
    );
    assert!(
        !all.iter().any(|heir| heir.starts_with("fromLib ")),
        "{all:?}"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A function that gives a class is an heir of it")]
fn a_function_that_gives_a_class_is_an_heir_of_it() -> Result<(), Box<dyn Error>> {
    let json = run("factory-members")?;
    let all = heirs(&json, "src/heirs.ts");
    for pair in [
        "typed of Service",
        "promised of Service",
        "nullable of Service",
        "untyped of Service",
        "arrow of Service",
        "awaited of Service",
        "expression of Service",
        "deep of Service",
        "either of Service",
        "either of Other",
    ] {
        assert!(all.iter().any(|heir| heir == pair), "{pair}: {all:?}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A function that gives a class only from a nested function is no heir"
)]
fn a_function_that_gives_a_class_only_from_a_nested_function_is_no_heir()
-> Result<(), Box<dyn Error>> {
    let json = run("factory-members")?;
    let all = heirs(&json, "src/heirs.ts");
    for name in ["nested", "unbound", "many"] {
        assert!(
            !all.iter().any(|heir| heir.starts_with(&format!("{name} "))),
            "{name} gives no class: {all:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A top-level value made by a call is an heir of what it calls"
)]
fn a_top_level_value_made_by_a_call_is_an_heir_of_what_it_calls() -> Result<(), Box<dyn Error>> {
    let json = run("factory-members")?;
    let all = heirs(&json, "src/heirs.ts");
    for pair in [
        "made of typed",
        "waited of promised",
        "fromStatic of Service",
    ] {
        assert!(all.iter().any(|heir| heir == pair), "{pair}: {all:?}");
    }
    for name in ["unknown", "fromLower"] {
        assert!(
            !all.iter().any(|heir| heir.starts_with(&format!("{name} "))),
            "{name} takes no head: {all:?}"
        );
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A function required with CommonJS is a use of its class"
)]
fn a_function_required_with_commonjs_is_a_use_of_its_class() -> Result<(), Box<dyn Error>> {
    let json = run("factory-members")?;
    for (name, call) in [("named", "makeService"), ("namespaced", "lib.makeService")] {
        let used = uses(&json, "src/commonjs.js", name).ok_or("no declaration")?;
        assert!(used.contains(&call), "{name}: {used:?}");
    }
    let read = reads(&json, "src/commonjs.js");
    assert!(read.iter().any(|read| read == "place"), "{read:?}");
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A field annotated or constructed with a class holds a value of it"
)]
fn a_field_annotated_or_constructed_with_a_class_holds_a_value_of_it() -> Result<(), Box<dyn Error>>
{
    let json = run("symbols-holds")?;
    let all = holds(&json, "src/holders.ts");
    for pair in [
        "Field.logger of Logger",
        "Field.built of Store",
        "Field.maybe of Logger",
        "Field.#secret of Logger",
    ] {
        assert!(all.iter().any(|held| held == pair), "{pair}: {all:?}");
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A parameter property holds a value of its class")]
fn a_parameter_property_holds_a_value_of_its_class() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-holds")?;
    let all = holds(&json, "src/holders.ts");
    for pair in ["Field.injected of Logger", "Field.named of Store"] {
        assert!(all.iter().any(|held| held == pair), "{pair}: {all:?}");
    }
    assert!(
        !all.iter().any(|held| held.starts_with("Field.plain ")),
        "a plain parameter is no member: {all:?}"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An interface property holds a value of its class")]
fn an_interface_property_holds_a_value_of_its_class() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-holds")?;
    let all = holds(&json, "src/holders.ts");
    for pair in [
        "Shape.logger of Logger",
        "Shape.optional of Store",
        "Literal.logger of Logger",
    ] {
        assert!(all.iter().any(|held| held == pair), "{pair}: {all:?}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A getter or a method that returns a class holds a value of it"
)]
fn a_getter_or_a_method_that_returns_a_class_holds_a_value_of_it() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-holds")?;
    let all = holds(&json, "src/holders.ts");
    for pair in [
        "Field.current of Logger",
        "Field.fetch of Store",
        "Shape.load of Store",
        "Shape.later of Logger",
        "Shape.view of Logger",
    ] {
        assert!(all.iter().any(|held| held == pair), "{pair}: {all:?}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A primitive, a union of classes and the class itself hold nothing"
)]
fn a_primitive_a_union_of_classes_and_the_class_itself_hold_nothing() -> Result<(), Box<dyn Error>>
{
    let json = run("symbols-holds")?;
    assert_eq!(
        holds(&json, "src/holders.ts"),
        [
            "Field.#secret of Logger",
            "Field.built of Store",
            "Field.current of Logger",
            "Field.fetch of Store",
            "Field.injected of Logger",
            "Field.logger of Logger",
            "Field.maybe of Logger",
            "Field.named of Store",
            "Literal.logger of Logger",
            "Shape.later of Logger",
            "Shape.load of Store",
            "Shape.logger of Logger",
            "Shape.optional of Store",
            "Shape.view of Logger",
        ],
        "`Logger | Other`, `string`, `Date`, `Array<…>`, `number`, a type alias of a class, a method with no return type and the class itself hold nothing"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A member read right on another is read as a pair")]
fn a_member_read_right_on_another_is_read_as_a_pair() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-pairs")?;
    let all = pairs(&json, "src/pairs.ts");
    for pair in ["ctx.logger", "logger.error", "#cache.size", "size.valueOf"] {
        assert!(all.iter().any(|read| read == pair), "{pair}: {all:?}");
    }
    assert_eq!(
        all.iter()
            .filter(|read| read.as_str() == "ctx.logger")
            .count(),
        1,
        "`ctx.logger` comes from `this.ctx.logger` alone: the parameter `ctx` is a local, not a member"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A pair sees through a non-null assertion and optional chaining"
)]
fn a_pair_sees_through_a_non_null_assertion_and_optional_chaining() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-pairs")?;
    let all = pairs(&json, "src/pairs.ts");
    for pair in ["a.b", "c.d", "e.f", "f.g"] {
        assert!(all.iter().any(|read| read == pair), "{pair}: {all:?}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member read on what a call gives is read on the method"
)]
fn a_member_read_on_what_a_call_gives_is_read_on_the_method() -> Result<(), Box<dyn Error>> {
    let json = run("symbols-pairs")?;
    let all = pairs(&json, "src/pairs.ts");
    for pair in ["get.name", "get.other", "load.title"] {
        assert!(all.iter().any(|read| read == pair), "{pair}: {all:?}");
    }
    assert!(
        !all.iter().any(|read| read == "x.get" || read == "x.a"),
        "the name at the root of a chain is no `on`: {all:?}"
    );
    assert!(
        !all.iter()
            .any(|read| read == "keys.length" || read == "parse.a" || read == "env.HOME"),
        "what is read on a builtin makes no pair: {all:?}"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A use keeps the line it is written on")]
fn a_use_keeps_the_line_it_is_written_on() -> Result<(), Box<dyn Error>> {
    let json = run("use-lines")?;

    let lines = use_lines(&json, "src/uses.ts", "run").unwrap();
    let line_of = |name: &str| {
        lines
            .iter()
            .find(|(used, _)| *used == name)
            .map(|(_, line)| *line)
    };
    assert_eq!(line_of("first"), Some(4), "{lines:?}");
    assert_eq!(line_of("second"), Some(5), "{lines:?}");
    // The chain starts on line 6 and its members stand on lines 7 and 8.
    assert_eq!(line_of("Builder.constructor"), Some(6), "{lines:?}");
    assert_eq!(line_of("Builder.add"), Some(7), "{lines:?}");
    assert_eq!(line_of("Builder.build"), Some(8), "{lines:?}");
    Ok(())
}
