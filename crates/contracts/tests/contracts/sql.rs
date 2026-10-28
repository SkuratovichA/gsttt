//! SQL files: what they declare, what they call, the dialect and the placeholders they are read with, and how a bad statement is told apart from the rest.

use gsttt_contracts::SQL;
use gsttt_core::connector::ConnectorError;
use gsttt_core::fact::ParseStatus;
use serde_json::json;

use crate::support::{HOST, Run, fixture, module, read_embedded, try_run, with_options};

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

#[gsttt_scenario::scenario("contract-formats", "A rename declares the new name")]
#[test]
fn a_rename_declares_the_new_name() {
    let run = Run::new("sql_rename", &[module(&SQL)]);
    assert_eq!(run.status("migrations/002_rename.sql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("migrations/002_rename.sql"),
        vec![
            "declares table bookings @1",
            "declares table reservations @1"
        ]
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

/// The first statement is read again with its `%s` replaced, and the second with its `:owner` and a string that holds a doubled quote; the sites keep the lines the tables are written on.
#[gsttt_scenario::scenario(
    "contract-formats",
    "A statement across lines keeps the lines of its tables"
)]
#[test]
fn a_statement_across_lines_keeps_the_lines_of_its_tables() {
    let run = Run::new("sql_multiline", &[module(&SQL)]);
    assert_eq!(run.status("queries.sql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("queries.sql"),
        vec![
            "calls table bookings @2",
            "calls table booking_items @3",
            "calls table carts @7",
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

/// An unterminated string swallows the text up to the end of the file for the tokenizer, so the statement after the bad one is only read when reading resumes after the next semicolon.
#[gsttt_scenario::scenario(
    "contract-formats",
    "A statement that does not tokenize does not hide the rest"
)]
#[test]
fn a_statement_that_does_not_tokenize_does_not_hide_the_rest() {
    let run = Run::new("sql_unterminated", &[module(&SQL)]);
    assert_eq!(
        run.status("queries.sql"),
        &ParseStatus::Recovered {
            errors: 1,
            first_error_line: 2
        }
    );
    assert_eq!(
        run.sites("queries.sql"),
        vec!["calls table a @1", "calls table c @3"]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Embedded SQL reports lines within the document")]
#[test]
fn embedded_sql_reports_lines_within_the_document() {
    // The document starts on line 7 of its host file; the engine, not the connector, moves its lines there.
    assert_eq!(
        read_embedded(&SQL, "sql_embedded", "sql", 7, "SELECT id FROM bookings"),
        vec!["calls table bookings @1"]
    );
    // A statement further down reports its own line within the document, also when it is read with its placeholder replaced.
    let text = "SELECT 1;\nSELECT id FROM bookings WHERE a = %s";
    assert_eq!(
        read_embedded(&SQL, "sql_embedded", "sql", 7, text),
        vec!["calls table bookings @2"]
    );
}

/// The tree holds one Python file and no SQL file, so the connector has only the embedded document to read. The snapshot shows the lines as the engine places them, so it changes when the engine moves them to the host file.
#[gsttt_scenario::scenario("contract-formats", "Embedded SQL is read without an SQL file")]
#[test]
fn embedded_sql_is_read_without_an_sql_file() {
    let run = Run::new("sql_embedded", &[module(&HOST), module(&SQL)]);
    let connectors: Vec<&str> = run
        .output
        .projects
        .iter()
        .map(|project| project.connector.name)
        .collect();
    assert_eq!(connectors, vec!["host", "sql"]);
    assert_eq!(
        run.embedded("app/repo.py"),
        vec![(Some("sql"), vec!["calls table bookings".to_owned()])]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
