//! The report of a project, from the built-in packs and from none.

use gsttt_config::builtin_packs;
use gsttt_core::connector::Provide;

use super::provides;
use crate::PACKS;

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
    }
}

#[test]
fn the_report_is_the_same_whatever_order_the_packs_come_in() {
    let mut packs = builtin_packs(PACKS).unwrap();
    let forward = provides(&packs);
    packs.reverse();
    assert_eq!(provides(&packs), forward);
}
