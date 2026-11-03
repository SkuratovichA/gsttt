//! What a file exports itself, when the connector cannot tell, and what a star passes on.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{
    affected_by, callers_of, exported_as, exports, imports, run, sites, status, uses,
};

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
        "src/callback.js",
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
#[scenario(
    "typescript",
    "A computed key as a statement of the top level keeps the exports known"
)]
fn a_computed_key_as_a_statement_of_the_top_level_keeps_the_exports_known()
-> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-computed")?;
    for path in ["src/top.js", "src/call.js"] {
        assert_eq!(status(&json, path), "ok", "{path}");
        assert_eq!(exports(&json, path), Some(vec![]), "{path}");
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A literal computed key exports its name")]
fn a_literal_computed_key_exports_its_name() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-computed")?;
    assert_eq!(exports(&json, "src/literal.js"), Some(vec!["a", "b"]));
    Ok(())
}

#[test]
#[scenario("typescript", "A computed key exports the names its value hands out")]
fn a_computed_key_exports_the_names_its_value_hands_out() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-computed")?;
    assert_eq!(
        exports(&json, "src/object.js"),
        Some(vec!["default", "helper", "other", "plain"])
    );
    assert_eq!(exports(&json, "src/identifier.js"), Some(vec!["internals"]));
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A computed key inside a loop or a function leaves the exports unknown"
)]
fn a_computed_key_inside_a_loop_or_a_function_leaves_the_exports_unknown()
-> Result<(), Box<dyn Error>> {
    let json = run("commonjs-exports-open")?;
    for path in ["src/indexed.js", "src/module-indexed.js", "src/callback.js"] {
        assert_eq!(exports(&json, path), None, "{path}");
    }
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

#[test]
fn the_heads_of_a_class_and_the_mounts_are_found_through_the_same_stars_aliases_and_defaults()
-> Result<(), Box<dyn Error>> {
    // The walk resolves the head of `implements` in `query/walk/overrides.rs` and the linker resolves a mount in `link.rs::export_of`, and the two follow a star, an alias and a default alike.
    assert_eq!(
        sites("star-heads")?,
        [
            "serves http GET /direct/legacy @7 in src/routes.ts",
            "serves http GET /members/{} @4 in src/routes.ts",
        ],
        "a name behind a star and an alias is mounted, and the default behind a star is not"
    );
    for (file, name, caller, reason) in [
        (
            "src/via-star.ts",
            "ViaStar.run",
            "src/by-run.ts",
            "the head behind a star",
        ),
        (
            "src/via-alias.ts",
            "ViaAlias.run",
            "src/by-run.ts",
            "the head behind an alias",
        ),
        (
            "src/via-default.ts",
            "ViaDefault.stop",
            "src/by-stop.ts",
            "the default of the file that declares it",
        ),
    ] {
        assert!(
            callers_of("star-heads", file, name)?.contains(&caller.to_owned()),
            "{reason}"
        );
    }
    assert!(
        !callers_of(
            "star-heads",
            "src/via-star-default.ts",
            "ViaStarDefault.stop"
        )?
        .contains(&"src/by-stop.ts".to_owned()),
        "a star forwards no default"
    );
    Ok(())
}

/// The questions asked of a tree of barrels, as a file and a name of it.
const BARREL_QUESTIONS: [(&str, &str); 5] = [
    ("src/lib/one.js", "one"),
    ("src/lib/many.js", "helper"),
    ("src/lib/many.js", "other"),
    ("src/lib/validate.js", "check"),
    ("src/lib/validate.js", "skip"),
];

/// Whether `fixture` lists `path` among the files that `impact` gives for `name` of `file`.
fn lists(fixture: &str, file: &str, name: &str, path: &str) -> Result<bool, Box<dyn Error>> {
    Ok(callers_of(fixture, file, name)?.contains(&path.to_owned()))
}

#[test]
#[scenario(
    "typescript",
    "A property whose value is a require passes the module's value on"
)]
fn a_property_whose_value_is_a_require_passes_the_modules_value_on() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-barrel")?;
    assert_eq!(
        imports(&json, "src/prop.js"),
        [
            "./lib/one re-export @1 [default] -> file src/lib/one.js",
            "./lib/many re-export @1 [default] -> file src/lib/many.js",
        ],
        "`{{ one: require('./lib/one') }}` re-exports `default` as `one`, and no load is left"
    );
    assert_eq!(
        exported_as(&json, "src/prop.js"),
        ["./lib/one default as one", "./lib/many default as many"]
    );
    assert!(
        lists("commonjs-barrel", "src/lib/one.js", "one", "src/by-prop.js")?,
        "the importer of `one` is a caller of the function"
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An assignment to exports of a require passes the module's value on"
)]
fn an_assignment_to_exports_of_a_require_passes_the_modules_value_on() -> Result<(), Box<dyn Error>>
{
    let json = run("commonjs-barrel")?;
    assert_eq!(
        imports(&json, "src/assigned.js"),
        ["./lib/one re-export @1 [default] -> file src/lib/one.js"],
        "`exports.one = require('./lib/one')`"
    );
    assert_eq!(exports(&json, "src/assigned.js"), Some(vec!["one"]));
    assert!(lists(
        "commonjs-barrel",
        "src/lib/one.js",
        "one",
        "src/by-assigned.js"
    )?);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A member of a required module passes that name on and no other"
)]
fn a_member_of_a_required_module_passes_that_name_on_and_no_other() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-barrel")?;
    assert_eq!(
        exported_as(&json, "src/member.js"),
        [
            "./lib/validate check as check",
            "./lib/many helper as helper"
        ],
        "`validate.check` under `const validate = require(…)` and `require('./lib/many').helper`"
    );
    for (file, name, caller, listed) in [
        ("src/lib/validate.js", "check", "src/by-member.js", true),
        ("src/lib/many.js", "helper", "src/by-member.js", true),
        ("src/lib/many.js", "other", "src/by-member.js", false),
        ("src/lib/validate.js", "skip", "src/by-member.js", false),
        ("src/lib/many.js", "other", "src/member.js", false),
    ] {
        assert_eq!(
            lists("commonjs-barrel", file, name, caller)?,
            listed,
            "{name} of {file} and {caller}"
        );
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A required module handed on whole passes every name and the value"
)]
fn a_required_module_handed_on_whole_passes_every_name_and_the_value() -> Result<(), Box<dyn Error>>
{
    let json = run("commonjs-barrel")?;
    assert_eq!(
        imports(&json, "src/whole.js"),
        [
            "./lib/many re-export-all @1 -> file src/lib/many.js",
            "./lib/many re-export @1 [default] -> file src/lib/many.js",
        ],
        "`module.exports = require('./lib/many')`"
    );
    assert_eq!(
        exports(&json, "src/whole.js"),
        None,
        "the exports of the file stay unknown"
    );
    assert!(lists(
        "commonjs-barrel",
        "src/lib/many.js",
        "helper",
        "src/by-whole.js"
    )?);
    assert!(!lists(
        "commonjs-barrel",
        "src/lib/many.js",
        "other",
        "src/by-whole.js"
    )?);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A spread and Object.assign of a require pass every name"
)]
fn a_spread_and_object_assign_of_a_require_pass_every_name() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-barrel")?;
    for (path, spec) in [
        ("src/spread.js", "./lib/many"),
        ("src/assign.js", "./lib/validate"),
    ] {
        assert_eq!(
            imports(&json, path),
            [format!(
                "{spec} re-export-all @1 -> file src/lib/{}.js",
                spec.trim_start_matches("./lib/")
            )],
            "{path}"
        );
        assert_eq!(exports(&json, path), None, "{path} stays unknown");
    }
    assert!(lists(
        "commonjs-barrel",
        "src/lib/many.js",
        "helper",
        "src/by-spread.js"
    )?);
    assert!(lists(
        "commonjs-barrel",
        "src/lib/validate.js",
        "check",
        "src/by-assign.js"
    )?);
    Ok(())
}

#[test]
#[scenario("typescript", "A spread gives no use to a neighbouring key")]
fn a_spread_gives_no_use_to_a_neighbouring_key() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-barrel")?;
    assert_eq!(
        uses(&json, "src/spread.js", "extra"),
        Some(vec![]),
        "`extra` of `{{ ...require('./lib/many'), extra: 1 }}` uses nothing"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A require that passes names on is no load")]
fn a_require_that_passes_names_on_is_no_load() -> Result<(), Box<dyn Error>> {
    let json = run("commonjs-barrel")?;
    for path in [
        "src/prop.js",
        "src/assigned.js",
        "src/whole.js",
        "src/spread.js",
    ] {
        assert!(
            imports(&json, path)
                .iter()
                .all(|import| !import.contains(" require ")),
            "{path} loads nothing"
        );
    }
    assert_eq!(
        imports(&json, "src/member.js")
            .iter()
            .filter(|import| import.contains(" require "))
            .count(),
        1,
        "only the `require` that binds `validate` stays a load"
    );
    // A name the barrel does not pass on leaves it unlisted.
    for (file, name, barrel) in [
        ("src/lib/one.js", "one", "src/assigned.js"),
        ("src/lib/many.js", "other", "src/prop.js"),
        ("src/lib/many.js", "other", "src/member.js"),
        ("src/lib/validate.js", "skip", "src/member.js"),
    ] {
        let listed = lists("commonjs-barrel", file, name, barrel)?;
        assert_eq!(listed, file.ends_with("one.js"), "{barrel} for {name}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A file asked about with no name lists its CommonJS barrels"
)]
fn a_file_asked_about_with_no_name_lists_its_commonjs_barrels() -> Result<(), Box<dyn Error>> {
    let affected = affected_by("commonjs-barrel", "src/lib/many.js")?;
    for barrel in [
        "src/prop.js 1 re-exports default as many from src/lib/many.js",
        "src/member.js 1 re-exports helper from src/lib/many.js",
        "src/whole.js 1 re-exports default, helper, other from src/lib/many.js",
        "src/spread.js 1 re-exports helper, other from src/lib/many.js",
    ] {
        assert!(affected.contains(&barrel.to_owned()), "{barrel}");
    }
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A CommonJS barrel answers as the same barrel written with export"
)]
fn a_commonjs_barrel_answers_as_the_same_barrel_written_with_export() -> Result<(), Box<dyn Error>>
{
    // Two trees of the same barrels, one written with `require` and one with `export … from`, which the walk of `impact` reads alike.
    for (file, name) in BARREL_QUESTIONS {
        assert_eq!(
            callers_of("commonjs-barrel", file, name)?,
            callers_of("commonjs-barrel-export", file, name)?,
            "{name} of {file}"
        );
    }
    assert_eq!(
        affected_by("commonjs-barrel", "src/lib/many.js")?,
        affected_by("commonjs-barrel-export", "src/lib/many.js")?
    );
    Ok(())
}
