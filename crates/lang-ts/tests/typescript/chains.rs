//! Member chains of any length, matched against the roots that rules list.

use std::error::Error;
use std::fs;
use std::time::Duration;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, problems, run_tree_within};

/// The members after `process.env.PORT`: far more than the roots of any rule have segments.
const MEMBERS: usize = 300;

#[scenario("typescript", "A long chain still matches a root of a rule")]
#[test]
fn a_long_chain_still_matches_a_root_of_a_rule() -> Result<(), Box<dyn Error>> {
    let tree = tempfile::tempdir()?;
    fs::write(tree.path().join("package.json"), r#"{ "name": "chain" }"#)?;
    fs::write(
        tree.path().join("use.ts"),
        format!(
            "export const x = process.env.PORT{};\n",
            ".a".repeat(MEMBERS)
        ),
    )?;
    let json = run_tree_within(tree.path(), Duration::from_secs(20))?;
    assert_eq!(contracts(&json, "use.ts"), ["calls env PORT @1"]);
    assert_eq!(problems(&json), NOTHING);
    Ok(())
}
