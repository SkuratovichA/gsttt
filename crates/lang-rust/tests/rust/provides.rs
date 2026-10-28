//! What each project reports it provides, on the `packs` fixture: `crates/api` depends on axum and `crates/cli` on
//! nothing a pack looks for.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;

use crate::support::index;

#[scenario("rust", "Blind spots are reported")]
#[test]
fn blind_spots_are_reported() {
    let out = index("packs");
    let roots = out.roots();
    assert!(
        roots.len() > 2,
        "the fixture should open several projects, not {roots:?}"
    );
    for root in roots {
        let blind_spots: Vec<String> = out
            .provides(&root)
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|entry| entry["blind_spots"].as_array().unwrap())
            .map(|spot| spot.as_str().unwrap().to_owned())
            .collect();
        for named in [
            "actix `configure`",
            "tonic services",
            "Diesel `table!`",
            "routers passed as values the connector cannot follow",
        ] {
            assert!(
                blind_spots.iter().any(|spot| spot.contains(named)),
                "the project at `{root}` does not name {named} among its blind spots: {blind_spots:#?}"
            );
        }
    }
}

#[test]
fn a_project_provides_what_the_packs_its_manifest_switches_on_find() {
    let out = index("packs");
    assert_json_snapshot!(out.provides("crates/api"));
}

#[test]
fn a_project_with_no_pack_for_a_contract_still_names_what_it_cannot_see() {
    let out = index("packs");
    assert_json_snapshot!(out.provides("crates/cli"));
}
