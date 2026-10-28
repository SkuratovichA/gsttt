//! Where specifiers lead: sources, aliases, workspace packages, built entries, packages and builtins.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{imports, leads, linked, project_of, provides, roots, run};

#[test]
#[scenario("typescript", "Relative specifiers find TypeScript sources")]
fn relative_specifiers_find_typescript_sources() -> Result<(), Box<dyn Error>> {
    let json = run("relative-specifiers")?;
    assert_eq!(
        imports(&json, "src/a.ts"),
        [
            "./util static @1 [util] -> file src/util.ts",
            "./util.js static @2 [again] -> file src/util.ts",
            "./components static @3 [Panel] -> file src/components/index.tsx",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Path aliases follow the config chain")]
fn path_aliases_follow_the_config_chain() -> Result<(), Box<dyn Error>> {
    let json = run("path-aliases")?;
    assert_eq!(
        imports(&json, "apps/web/src/page.ts"),
        ["@db/client static @1 [client] -> file packages/db/src/client.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Workspace packages resolve to their sources")]
fn workspace_packages_resolve_to_their_sources() -> Result<(), Box<dyn Error>> {
    let json = run("workspace-packages")?;
    assert_eq!(
        imports(&json, "apps/web/src/main.ts"),
        ["@acme/ui/button static @1 [Button] -> file packages/ui/src/button.tsx"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Built entries map back to sources")]
fn built_entries_map_back_to_sources() -> Result<(), Box<dyn Error>> {
    let json = run("built-entries")?;
    assert_eq!(
        imports(&json, "packages/api/src/main.ts"),
        ["@acme/db static @1 [db] -> file packages/db/src/index.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Packages, builtins and unknown names are told apart")]
fn packages_builtins_and_unknown_names_are_told_apart() -> Result<(), Box<dyn Error>> {
    let json = run("packages-and-builtins")?;
    assert_eq!(
        imports(&json, "apps/web/src/main.ts"),
        [
            "react static @1 [useState] -> external react",
            "react-dom/client static @2 [createRoot] -> external react-dom",
            "node:fs static @3 [readFileSync] -> builtin",
            "path static @4 [join] -> builtin",
            "left-pad static @5 [default] -> unresolved unknown-package",
            "./nope static @6 [missing] -> unresolved not-found",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A marker package.json is no project")]
fn a_marker_package_json_is_no_project() -> Result<(), Box<dyn Error>> {
    let json = run("nested-manifests")?;
    assert_eq!(roots(&json), [".", "packages/app"]);
    for path in [
        "packages/app/src/esm/entry.ts",
        "packages/app/src/esm/package.json",
    ] {
        assert_eq!(project_of(&json, path), Some("ts packages/app"), "{path}");
    }
    let expected = [
        ("react", "external react"),
        ("lodash", "external lodash"),
        ("uuid", "external uuid"),
        ("left-pad", "unresolved unknown-package"),
    ];
    for (spec, target) in expected {
        let from = "packages/app/src/esm/entry.ts";
        assert_eq!(leads(&json, from, spec), target, "{spec}");
    }
    assert_eq!(
        leads(&json, "packages/app/src/index.ts", "lodash"),
        "external lodash"
    );
    assert!(
        provides(&json, "packages/app")
            .iter()
            .any(|line| line.starts_with("http serves"))
    );
    assert!(
        linked(&json)
            .iter()
            .any(|fact| fact.ends_with("in packages/app/src/esm/server.ts"))
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A solution tsconfig hands a file to the config that lists it"
)]
fn a_solution_tsconfig_hands_a_file_to_the_config_that_lists_it() -> Result<(), Box<dyn Error>> {
    let json = run("solution-configs")?;
    assert_eq!(
        leads(&json, "src/main.ts", "@/components/Button"),
        "file src/components/Button.ts"
    );
    assert_eq!(
        leads(&json, "vite.config.ts", "@/Plugin"),
        "file tooling/Plugin.ts"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A jsconfig governs a directory without a tsconfig")]
fn a_jsconfig_governs_a_directory_without_a_tsconfig() -> Result<(), Box<dyn Error>> {
    let json = run("solution-configs")?;
    assert_eq!(
        leads(&json, "legacy/main.js", "~/util"),
        "file legacy/lib/util.js"
    );
    Ok(())
}

#[test]
fn an_extension_stands_for_the_files_a_build_may_have_written_it_from() -> Result<(), Box<dyn Error>>
{
    let json = run("extension-table")?;
    let expected = [
        ("./plain", "file src/plain.ts"),
        ("./tsx-only", "file src/tsx-only.tsx"),
        ("./types-only", "file src/types-only.d.ts"),
        ("./js-only", "file src/js-only.js"),
        ("./jsx-only", "file src/jsx-only.jsx"),
        ("./module-only", "unresolved not-found"),
        ("./common-only", "unresolved not-found"),
        ("./esm-only", "unresolved not-found"),
        ("./cjs-only", "unresolved not-found"),
        ("./built.js", "file src/built.ts"),
        ("./esm.mjs", "file src/esm.mts"),
        ("./cjs.cjs", "file src/cjs.cts"),
        ("./comp.jsx", "file src/comp.tsx"),
        ("./decl.d.ts", "file src/decl.d.ts"),
        ("./dir", "file src/dir.ts"),
        ("./dir/", "file src/dir/index.ts"),
        (".", "file src/index.ts"),
        ("..", "file index.ts"),
    ];
    for (spec, target) in expected {
        assert_eq!(leads(&json, "src/main.ts", spec), target, "{spec}");
    }
    Ok(())
}

#[test]
fn the_longest_prefix_wins_and_the_first_listed_of_equals() -> Result<(), Box<dyn Error>> {
    let json = run("alias-priority")?;
    let expected = [
        ("@a/x.js", "file one/x.ts"),
        ("@exact", "file exact/index.ts"),
        ("@lib/thing", "file lib/thing.ts"),
        ("@long/deep/a", "file deep/a.ts"),
        ("@long/other", "file shallow/other.ts"),
    ];
    for (spec, target) in expected {
        assert_eq!(leads(&json, "src/main.ts", spec), target, "{spec}");
    }
    Ok(())
}

#[test]
fn options_set_only_in_a_base_reach_a_file_through_a_package() -> Result<(), Box<dyn Error>> {
    let json = run("config-chain")?;
    let from = "apps/web/src/main.ts";
    assert_eq!(leads(&json, from, "@lib/thing"), "file libs/thing.ts");
    assert_eq!(leads(&json, from, "shared/util"), "file shared/util.ts");
    Ok(())
}

#[test]
#[scenario("typescript", "An exports map hides what it does not list")]
fn an_exports_map_hides_what_it_does_not_list() -> Result<(), Box<dyn Error>> {
    let json = run("package-exports")?;
    let expected = [
        ("lib/feature/one", "file packages/lib/src/feature/one.ts"),
        ("lib/internal/secret", "unresolved not-found"),
        ("lib/hidden", "unresolved not-found"),
        ("lib/unlisted", "unresolved not-found"),
    ];
    for (spec, target) in expected {
        let from = "apps/web/src/esm.ts";
        assert_eq!(leads(&json, from, spec), target, "{spec}");
    }
    Ok(())
}

#[test]
#[scenario("typescript", "Conditions follow the form of the import")]
fn conditions_follow_the_form_of_the_import() -> Result<(), Box<dyn Error>> {
    let json = run("package-exports")?;
    let expected = [
        ("lib", "file packages/lib/src/esm.ts"),
        ("typed", "file packages/typed/src/types.d.ts"),
        ("#dep", "file apps/web/src/dep-esm.ts"),
        ("#fs", "builtin"),
        ("#utils/format", "file apps/web/src/utils/format.ts"),
    ];
    for (spec, target) in expected {
        let from = "apps/web/src/esm.ts";
        assert_eq!(leads(&json, from, spec), target, "{spec}");
    }
    assert_eq!(
        leads(&json, "apps/web/src/cjs.ts", "lib"),
        "file packages/lib/src/cjs.ts"
    );
    Ok(())
}

#[test]
fn a_package_with_a_types_field_resolves_to_the_declaration_file() -> Result<(), Box<dyn Error>> {
    let json = run("package-exports")?;
    assert_eq!(
        leads(&json, "apps/web/src/esm.ts", "fields"),
        "file packages/fields/types/index.d.ts"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An import of an ignored file says so")]
fn an_import_of_an_ignored_file_says_so() -> Result<(), Box<dyn Error>> {
    let json = run("ignored-targets")?;
    let expected = [
        ("../generated/client", "unresolved ignored"),
        ("@gen/client", "unresolved ignored"),
        ("./absent", "unresolved not-found"),
        ("./folder", "unresolved not-found"),
    ];
    for (spec, target) in expected {
        assert_eq!(leads(&json, "src/main.ts", spec), target, "{spec}");
    }
    Ok(())
}
