//! Where specifiers lead: sources, aliases, workspace packages, built entries, packages and builtins.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{imports, run};

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
