//! The report of a project, from the built-in packs and from none.

use gsttt_config::builtin_packs;
use gsttt_core::connector::Provide;

use super::provides;
use crate::PACKS;
use crate::link::{MAX_DEPTH, MAX_VISITS};
use crate::tree::MAX_MODULES;

/// What one entry says before its blind spots: `<kind> <role> via <packs>`.
fn headline(entry: &Provide) -> String {
    format!(
        "{} {} via [{}]",
        entry.kind.as_str(),
        entry.role.as_str(),
        entry.via.join(", ")
    )
}

#[test]
fn every_pack_is_credited_with_the_contracts_its_rules_name() {
    let packs = builtin_packs(PACKS).unwrap();
    let report: Vec<String> = provides(&packs).iter().map(headline).collect();
    assert_eq!(
        report,
        [
            "http serves via [actix, axum]",
            "http calls via [reqwest]",
            "grpc serves via []",
            "table declares via [sea_orm]",
            "table calls via [sqlx]",
            "env calls via [env]",
        ]
    );
}

#[test]
fn a_project_without_packs_still_reports_what_it_cannot_see() {
    let report = provides(&[]);
    let report: Vec<String> = report.iter().map(headline).collect();
    assert_eq!(
        report,
        [
            "http serves via []",
            "http calls via []",
            "grpc serves via []",
            "table declares via []",
            "table calls via []",
            "env calls via []",
        ]
    );
}

#[test]
fn every_entry_says_what_it_does_not_see() {
    let packs = builtin_packs(PACKS).unwrap();
    for entry in provides(&packs) {
        assert!(
            !entry.blind_spots.is_empty(),
            "{} has no blind spot",
            headline(&entry)
        );
        assert!(
            entry
                .blind_spots
                .last()
                .is_some_and(|spot| spot.contains("`include!`")),
            "{} does not name generated code",
            headline(&entry)
        );
        assert!(
            entry
                .blind_spots
                .iter()
                .any(|spot| spot.contains("glob import")),
            "{} does not name glob imports",
            headline(&entry)
        );
    }
}

#[test]
fn the_report_is_the_same_whatever_order_the_packs_come_in() {
    let mut packs = builtin_packs(PACKS).unwrap();
    let forward = provides(&packs);
    packs.reverse();
    assert_eq!(provides(&packs), forward);
}

#[gsttt_scenario::scenario("rust", "A cap is named among the blind spots")]
#[test]
fn a_cap_is_named_among_the_blind_spots() {
    // Without packs too: a project says what it cannot follow whether or not it can find routes.
    let report = provides(&[]);
    let serving = report
        .iter()
        .find(|entry| headline(entry).starts_with("http serves"))
        .unwrap();
    for cap in [
        format!("more than {MAX_DEPTH} deep"),
        format!("more than {MAX_VISITS} times"),
    ] {
        assert!(
            serving.blind_spots.iter().any(|spot| spot.contains(&cap)),
            "the router caps are not named: `{cap}`"
        );
    }
    let modules = format!("more than {MAX_MODULES} modules");
    for entry in &report {
        assert!(
            entry.blind_spots.iter().any(|spot| spot.contains(&modules)),
            "{} does not name the module cap",
            headline(entry)
        );
    }
}
