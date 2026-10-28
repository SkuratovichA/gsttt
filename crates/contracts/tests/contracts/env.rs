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

#[gsttt_scenario::scenario(
    "contract-formats",
    "A quoted value over several lines declares its name once"
)]
#[test]
fn a_quoted_value_over_several_lines_declares_its_name_once() {
    let run = Run::new("env_multiline", &[module(&ENV)]);
    assert_eq!(run.status(".env.example"), &ParseStatus::Ok);
    assert_eq!(
        run.sites(".env.example"),
        vec![
            "declares env PRIVATE_KEY @2",
            "declares env GREETING @5",
            "declares env PORT @7"
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario(
    "contract-formats",
    "A source file named like an env file is not an env file"
)]
#[test]
fn a_source_file_named_like_an_env_file_is_not_an_env_file() {
    let run = Run::new("env_names", &[module(&ENV)]);
    assert_eq!(run.unclaimed(), vec![".env.d.ts", "types/.env.schema.ts"]);
    for name in [".env.local", ".env.production.local", "staging.env"] {
        assert_eq!(run.project_of(name), "env", "{name}");
    }
    insta::assert_json_snapshot!(run.snapshot());
}
