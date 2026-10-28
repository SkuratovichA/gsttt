//! SQL files: what they declare, what they call, the dialect and the placeholders they are read with, and how a
//! bad statement is told apart from the rest.

use gsttt_contracts::SQL;
use gsttt_core::connector::ConnectorError;
use gsttt_core::fact::ParseStatus;
use serde_json::json;

use crate::support::{HOST, Run, fixture, module, try_run, with_options};

#[gsttt_scenario::scenario("contract-formats", "A migration declares its tables")]
#[test]
fn a_migration_declares_its_tables() {
    let run = Run::new("sql_migration", &[module(&SQL)]);
    assert_eq!(run.status("migrations/001_init.sql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("migrations/001_init.sql"),
        vec![
            "declares table bookingitem @1",
            "declares table bookings @2",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Queries call their tables")]
#[test]
fn queries_call_their_tables() {
    let run = Run::new("sql_queries", &[module(&SQL)]);
    assert_eq!(run.status("queries.sql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("queries.sql"),
        vec!["calls table booking_items @1", "calls table bookings @1"]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "The dialect option picks the grammar")]
#[test]
fn the_dialect_option_picks_the_grammar() {
    let mysql = with_options(&SQL, json!({ "dialect": "mysql" }));
    let run = Run::new("sql_mysql", &[mysql]);
    assert_eq!(run.status("queries.sql"), &ParseStatus::Ok);
    assert_eq!(run.sites("queries.sql"), vec!["calls table bookings @1"]);

    // The default grammar is PostgreSQL's, where a backtick opens nothing a statement can use.
    let default = Run::new("sql_mysql", &[module(&SQL)]);
    assert!(matches!(
        default.status("queries.sql"),
        ParseStatus::Recovered { .. } | ParseStatus::Failed(_)
    ));
}

#[gsttt_scenario::scenario("contract-formats", "An unknown dialect is an options error")]
#[test]
fn an_unknown_dialect_is_an_options_error() {
    let repo = fixture("sql_queries");
    let cobol = with_options(&SQL, json!({ "dialect": "cobol" }));
    let error = try_run(&repo, &[cobol]).unwrap_err();
    let ConnectorError::Options { connector, message } = &error else {
        panic!("expected an options error, got {error:?}");
    };
    assert_eq!(connector, "sql");
    assert!(message.contains("dialect"), "{message}");
    assert!(message.contains("cobol"), "{message}");
}

#[gsttt_scenario::scenario("contract-formats", "Driver placeholders are parameters")]
#[test]
fn driver_placeholders_are_parameters() {
    let run = Run::new("sql_placeholders", &[module(&SQL)]);
    assert_eq!(run.status("queries.sql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("queries.sql"),
        vec![
            "calls table bookings @1",
            "calls table bookings @2",
            "calls table bookings @3",
            "calls table bookings @4",
            "calls table bookings @5",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "One bad statement does not hide the rest")]
#[test]
fn one_bad_statement_does_not_hide_the_rest() {
    let run = Run::new("sql_recovery", &[module(&SQL)]);
    assert_eq!(
        run.status("queries.sql"),
        &ParseStatus::Recovered {
            errors: 1,
            first_error_line: 3
        }
    );
    assert_eq!(
        run.sites("queries.sql"),
        vec!["calls table bookings @1", "calls table carts @4"]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Embedded SQL documents keep host lines")]
#[test]
fn embedded_sql_documents_keep_host_lines() {
    let run = Run::new("sql_embedded", &[module(&HOST), module(&SQL)]);
    assert_eq!(
        run.embedded("app/repo.py"),
        vec![(Some("sql"), vec!["calls table bookings @7".to_owned()])]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
