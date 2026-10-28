//! Projects, configs, test tags and what a project provides.

use std::error::Error;

use gsttt_core::fact::FileTags;
use gsttt_scenario::scenario;

use crate::support::{
    NOTHING, blind_spots, contracts, imports, project_of, provides, roots, run, status, tags,
};

#[test]
#[scenario("typescript", "Every package is a project")]
fn every_package_is_a_project() -> Result<(), Box<dyn Error>> {
    let json = run("projects")?;
    assert_eq!(roots(&json), [".", "apps/web", "packages/ui"]);
    assert_eq!(
        project_of(&json, "packages/ui/src/button.tsx"),
        Some("ts packages/ui")
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A broken tsconfig fails alone")]
fn a_broken_tsconfig_fails_alone() -> Result<(), Box<dyn Error>> {
    let json = run("broken-tsconfig")?;
    assert_eq!(status(&json, "apps/web/tsconfig.json"), "failed syntax @3");
    assert_eq!(status(&json, "apps/web/src/page.ts"), "ok");
    assert_eq!(
        imports(&json, "apps/web/src/page.ts"),
        ["./util static @1 [util] -> file apps/web/src/util.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A config extends another")]
fn a_config_extends_another() -> Result<(), Box<dyn Error>> {
    let json = run("tsconfig-extends")?;
    assert_eq!(
        imports(&json, "apps/web/tsconfig.json"),
        ["../../tsconfig.base.json reference @3 -> file tsconfig.base.json"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Test files are tagged")]
fn files_of_tests_are_tagged() -> Result<(), Box<dyn Error>> {
    let json = run("test-files")?;
    for path in ["src/a.test.ts", "src/__tests__/b.ts", "test/c.ts"] {
        assert!(tags(&json, path).contains(FileTags::TEST), "{path}");
    }
    assert!(!tags(&json, "src/d.ts").contains(FileTags::TEST));
    assert!(tags(&json, "package.json").contains(FileTags::MANIFEST));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Packs follow the manifest")]
fn packs_follow_the_manifest() -> Result<(), Box<dyn Error>> {
    let json = run("pack-activation")?;
    assert_eq!(
        contracts(&json, "apps/api/src/status.controller.ts"),
        ["serves http GET /status/live @5"]
    );
    assert_eq!(
        contracts(&json, "apps/web/src/status.controller.ts"),
        NOTHING
    );
    assert!(provides(&json, "apps/api").contains(&"http serves via nestjs".to_owned()));
    assert!(!provides(&json, "apps/web").contains(&"http serves via nestjs".to_owned()));
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Blind spots are reported")]
fn blind_spots_are_reported() -> Result<(), Box<dyn Error>> {
    let json = run("blind-spots")?;
    assert_eq!(
        provides(&json, "."),
        [
            "http serves via express,nestjs",
            "http calls via fetch",
            "graphql serves via nestjs",
            "trpc serves via trpc",
            "trpc calls via trpc",
            "env calls via env",
        ]
    );
    let spots = blind_spots(&json, ".");
    assert!(spots.iter().any(|spot| spot.contains("file-system routes")));
    assert!(
        spots
            .iter()
            .any(|spot| spot.contains("routers mounted from another package"))
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}
