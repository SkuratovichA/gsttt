//! Env files.

use gsttt_contracts::ENV;
use gsttt_core::fact::ParseStatus;

use crate::support::{Run, module};

#[gsttt_scenario::scenario("contract-formats", "Example env files declare their variables")]
#[test]
fn example_env_files_declare_their_variables() {
    let run = Run::new("env_example", &[module(&ENV)]);
    assert_eq!(run.status(".env.example"), &ParseStatus::Ok);
    assert_eq!(
        run.sites(".env.example"),
        vec!["declares env DATABASE_URL @2", "declares env PORT @3"]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
